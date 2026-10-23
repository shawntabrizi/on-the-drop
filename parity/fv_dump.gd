extends SceneTree
## Parity harness, GDScript side. Runs the frame-visualizer analysis
## (lib/song_overview.gd + lib/song_map.gd, master) on each input and writes,
## per input <name>:
##   <out>/<name>.gd.json  the map (SongMap.to_dict fields plus downbeats,
##                         drops, part_bounds, section words, fixed_tempo,
##                         grid_drift), the overview levels, timings
##   <out>/<name>.bins     the band bins SongMap.analyse got (raw + air)
##   <out>/<name>.pcm      with --pcm: the decoded frames build() mixed
##                         (header: "OTDPCM1\0", rate f64, frames i64; then
##                         interleaved stereo f32)
## Inputs: WAV / MP3 files, "synth:<form>" (test/song_map_synth.gd) and
## "bins:<form>" (the band-bin forms of test/test_song_map.gd).
## Run headless and silent from a project that holds frame-visualizer's lib/:
##   godot --headless --audio-driver Dummy --path <fv-min> --script <this> -- --out <dir> [--pcm] <input> ...
## The outputs are audio-derived: keep them out of git.

const Synth := preload("res://test/song_map_synth.gd")
const BIN := 512.0 / 44100.0


class Capture:
	extends SongOverview
	var raw_bins := PackedFloat32Array()
	var air_bins := PackedFloat32Array()
	var nbins := 0
	var t_analyse := 0.0

	func analyse(raw: PackedFloat32Array, nb: int, air := PackedFloat32Array()) -> void:
		raw_bins = raw
		air_bins = air
		nbins = nb
		var t0 := Time.get_ticks_usec()
		super(raw, nb, air)
		t_analyse = (Time.get_ticks_usec() - t0) / 1000.0


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	var out := ""
	var pcm := false
	var inputs: Array[String] = []
	var i := 0
	while i < args.size():
		match args[i]:
			"--out":
				out = args[i + 1]
				i += 1
			"--pcm":
				pcm = true
			_:
				inputs.append(args[i])
		i += 1
	DirAccess.make_dir_recursive_absolute(out)
	var fails := 0
	for inp in inputs:
		if not _one(inp, out, pcm):
			print("fv_dump: FAIL %s" % inp)
			fails += 1
	print("fv_dump: %d inputs, %d failed" % [inputs.size(), fails])
	quit(1 if fails > 0 else 0)


func _synth(form: String) -> Dictionary:
	match form:
		"default": return Synth.make()
		"half": return Synth.make([0, 2], {"bpm": 117.3, "snare": [2], "hats8": true})
		"drift": return Synth.make([0, 1, 2, 3], {"drift": 0.03})
		"late": return Synth.make([0, 1, 2, 3], {"form": Synth.LATE_FORM})
		"fill": return Synth.make([0, 1, 2, 3], {"form": [[8, "intro"], [7, "drop1"], [1, "gap"], [8, "drop"], [8, "outro"]]})
		"repeat": return Synth.make([0, 1, 2, 3], {"form": Synth.REPEAT_FORM})
		"drops": return Synth.make([0, 1, 2, 3], {"form": Synth.DROPS_FORM})
		"loudbuild": return Synth.make([0, 1, 2, 3], {"form": Synth.LOUD_BUILD_FORM})
		"entry": return Synth.make([0, 1, 2, 3], {"form": Synth.ENTRY_FORM})
	return {}


func _one(inp: String, out: String, want_pcm: bool) -> bool:
	var name := inp.get_file().get_basename()
	var stream: AudioStream = null
	if inp.begins_with("bins:"):
		return _bins_only(inp.substr(5), out)
	if inp.begins_with("synth:"):
		name = "synth_" + inp.substr(6)
		var s := _synth(inp.substr(6))
		if s.is_empty():
			return false
		stream = s.wav
		# the synth's own 22050 Hz samples (before Godot's resampler), for the Rust synth port check
		var fw := FileAccess.open(out.path_join(name + ".s16"), FileAccess.WRITE)
		fw.store_buffer((s.wav as AudioStreamWAV).data)
	elif inp.get_extension().to_lower() == "wav":
		stream = AudioStreamWAV.load_from_file(inp)
	elif inp.get_extension().to_lower() == "mp3":
		stream = AudioStreamMP3.load_from_file(inp)
	if stream == null:
		return false
	var t_dec := 0.0
	if want_pcm:
		t_dec = _write_pcm(stream, 44100.0, out.path_join(name + ".pcm"))
	var o := Capture.new()
	var t0 := Time.get_ticks_usec()
	if not o.build(stream, 44100.0):
		return false
	var t_build := (Time.get_ticks_usec() - t0) / 1000.0
	_write_bins(o.raw_bins, o.air_bins, o.nbins, o.bin_seconds(), out.path_join(name + ".bins"))
	var d := _map_dict(o.map)
	d["levels"] = Array(o.levels)
	d["bins"] = o.bins
	d["ms_build"] = t_build  # decode + bins + map (as the app)
	d["ms_decode"] = t_dec  # a separate decode pass of the same stream (mix_audio only)
	d["ms_analyse"] = o.t_analyse  # SongMap.analyse (from bins)
	return _write_json(d, out.path_join(name + ".gd.json"))


## Mixes the stream as SongOverview.build does and writes the frames.
## Returns the mix time (ms), without the file write.
func _write_pcm(stream: AudioStream, rate: float, path: String) -> float:
	var t0 := Time.get_ticks_usec()
	var pb := stream.instantiate_playback()
	pb.start(0.0)
	var total := int(stream.get_length() * rate)
	var all := PackedVector2Array()
	var done := 0
	while done < total:
		var buf: PackedVector2Array = pb.mix_audio(1.0, mini(SongOverview.CHUNK, total - done))
		if buf.is_empty():
			break
		all.append_array(buf)
		done += buf.size()
	var ms := (Time.get_ticks_usec() - t0) / 1000.0
	var f := FileAccess.open(path, FileAccess.WRITE)
	f.store_buffer("OTDPCM1".to_ascii_buffer() + PackedByteArray([0]))
	f.store_double(rate)
	f.store_64(all.size())
	var fl := PackedFloat32Array()
	fl.resize(all.size() * 2)
	for i in all.size():
		fl[i * 2] = all[i].x
		fl[i * 2 + 1] = all[i].y
	f.store_buffer(fl.to_byte_array())
	return ms


func _write_bins(raw: PackedFloat32Array, air: PackedFloat32Array, nb: int, bin_s: float, path: String) -> void:
	var f := FileAccess.open(path, FileAccess.WRITE)
	f.store_buffer("OTDBIN1".to_ascii_buffer() + PackedByteArray([0]))
	f.store_64(nb)
	f.store_double(bin_s)
	f.store_64(air.size())
	f.store_buffer(raw.to_byte_array())
	f.store_buffer(air.to_byte_array())


func _bins_only(form: String, out: String) -> bool:
	var parts := []
	match form:
		"parts": parts = [[16, -12, -6, -20, -20, true], [16, -12, -20, -6, -6, true], [16, -12, -6, -20, -20, true]]
		"steady": parts = [[48, -6, -6, -8, -8, true]]
		"alt":
			for q in 12:
				parts.append([2, -12, -6 if q % 2 == 0 else -20, -20 if q % 2 == 0 else -6, -20 if q % 2 == 0 else -6, true])
		_: return false
	var d := _bins(parts)
	_write_bins(d.raw, d.air, d.nb, BIN, out.path_join("bins_" + form + ".bins"))
	var m := SongMap.new()
	var t0 := Time.get_ticks_usec()
	m.analyse(d.raw, d.nb, BIN, d.air)
	var j := _map_dict(m)
	j["ms_analyse"] = (Time.get_ticks_usec() - t0) / 1000.0
	return _write_json(j, out.path_join("bins_" + form + ".gd.json"))


## test/test_song_map.gd _bins, verbatim.
static func _bins(parts: Array) -> Dictionary:
	var beat := 0.5
	var bars := 0
	for p in parts:
		bars += p[0]
	var nb := int((1.5 + bars * 4 * beat) / BIN)
	var raw := PackedFloat32Array()
	raw.resize(nb * 3)
	var air := PackedFloat32Array()
	air.resize(nb)
	var starts := []
	var t0 := 0.5
	var rng := RandomNumberGenerator.new()
	rng.seed = 3
	for p in parts:
		starts.append(t0)
		var t1: float = t0 + p[0] * 4 * beat
		for i in range(int(t0 / BIN), mini(int(t1 / BIN), nb)):
			var ph := fmod(i * BIN - t0, beat) / beat
			var k := 0.5 * exp(-ph * 8.0) if p[5] else 0.0
			var j := 1.0 + 0.05 * rng.randf_range(-1.0, 1.0)
			raw[i * 3] = (db_to_linear(p[1]) * minf(ph * 4.0, 1.0) + k) * j
			raw[i * 3 + 1] = db_to_linear(p[2]) * j
			raw[i * 3 + 2] = (db_to_linear(p[3]) + 0.06 * k) * j
			air[i] = db_to_linear(p[4]) * (1.0 if ph > 0.5 and ph < 0.6 else 0.3) * j
		t0 = t1
	return {"raw": raw, "air": air, "nb": nb, "starts": starts}


func _map_dict(m: SongMap) -> Dictionary:
	var d := m.to_dict()
	for k in d.keys():
		var v = d[k]
		if v is PackedFloat32Array or v is PackedByteArray:
			d[k] = Array(v)
	d["finale"] = [m.finale_start, m.finale_end]
	d["downbeats"] = Array(m.downbeats)
	d["drops"] = Array(m.drops)
	d["part_bounds"] = Array(m.part_bounds)
	d["words"] = Array(m.section_words()) if not m.sections.is_empty() else []
	d["fixed_tempo"] = m.fixed_tempo
	d["grid_drift"] = m.grid_drift
	d["ms"] = m.ms
	return d


func _write_json(d: Dictionary, path: String) -> bool:
	var f := FileAccess.open(path, FileAccess.WRITE)
	if f == null:
		return false
	f.store_string(JSON.stringify(d, "", true, true))
	return true
