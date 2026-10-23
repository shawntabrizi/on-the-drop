extends Node
## Headless check of the on-the-drop GDExtension: the analysis and the
## track fingerprint, called as the app calls them.
## Prints "otd-spike: PASS" or "otd-spike: FAIL ..." and quits with 0 or 1.
## Calls go through ClassDB (not the OnTheDrop identifier) so this script
## still parses, and the app still exits by itself, when the library is missing.
## (The file and the "otd-spike" prefix keep the spike's names, which the
## scripts use.)

const EXPECTED_VERSION := "0.1.0"
const EXPECTED_ANALYSIS := 11
const RATE := 44100
const BPM := 120.0

var _fails: PackedStringArray = []


func _ready() -> void:
	print("otd-spike: Godot %s, OS %s, arch %s" % [
		Engine.get_version_info().string, OS.get_name(), Engine.get_architecture_name()])
	_run()
	if _fails.is_empty():
		print("otd-spike: PASS")
		get_tree().quit(0)
	else:
		print("otd-spike: FAIL %d: %s" % [_fails.size(), "; ".join(_fails)])
		get_tree().quit(1)


func _check(ok: bool, what: String) -> void:
	print("otd-spike:   %s %s" % ["ok  " if ok else "FAIL", what])
	if not ok:
		_fails.append(what)


func _call(f: String, args: Array) -> Variant:
	return ClassDB.class_call_static.callv(["OnTheDrop", f] + args)


## A beat track: a 55 Hz kick on every beat, a noise hat between, a chord
## pad that changes every bar; `lead` s of silence first, `secs` s long.
func _song(secs: float, lead: float, seed_: int, chord_hz: float) -> PackedVector2Array:
	var rng := RandomNumberGenerator.new()
	rng.seed = seed_
	var n := int((secs + lead) * RATE)
	var a := PackedVector2Array()
	a.resize(n)
	var beat := 60.0 / BPM
	for i in range(int(lead * RATE), n):
		var t := float(i) / RATE - lead
		var tb := fmod(t, beat)
		var th := fmod(t + beat * 0.5, beat)
		var bar := int(t / (beat * 4.0))
		var v := 0.8 * exp(-tb / 0.08) * sin(TAU * 55.0 * tb)
		v += 0.15 * exp(-th / 0.02) * rng.randf_range(-1.0, 1.0)
		v += 0.1 * sin(TAU * chord_hz * (1.0 + 0.25 * (bar % 3)) * t)
		a[i] = Vector2(v, v)
	return a


func _run() -> void:
	_check(ClassDB.class_exists("OnTheDrop"), "class OnTheDrop is registered")
	if not ClassDB.class_exists("OnTheDrop"):
		return
	var v: String = _call("version", [])
	# Why: the Godot side must load the core it was built with.
	_check(v == EXPECTED_VERSION, "version() == %s (got %s)" % [EXPECTED_VERSION, v])
	_check(_call("analysis_version", []) == EXPECTED_ANALYSIS, "analysis_version() == %d" % EXPECTED_ANALYSIS)

	var song := _song(40.0, 0.0, 1, 220.0)
	var t0 := Time.get_ticks_usec()
	var m: Dictionary = _call("analyze", [song, float(RATE)])
	print("otd-spike:   analyze 40 s: %.1f ms, bpm %.2f, %d beats" % [
		(Time.get_ticks_usec() - t0) / 1000.0, m.get("bpm", 0.0), m.get("beats", PackedFloat32Array()).size()])
	# Why: the app reads the map as SongMap.from_dict reads its cache; each
	# field must be there with the type from_dict expects.
	var types := {"duration": TYPE_FLOAT, "bpm": TYPE_FLOAT, "beats": TYPE_PACKED_FLOAT32_ARRAY,
		"beat_bar": TYPE_PACKED_BYTE_ARRAY, "beat_support": TYPE_PACKED_BYTE_ARRAY,
		"phrases": TYPE_PACKED_FLOAT32_ARRAY, "sections": TYPE_PACKED_FLOAT32_ARRAY,
		"finale": TYPE_VECTOR2, "energy": TYPE_PACKED_BYTE_ARRAY, "kicks": TYPE_PACKED_FLOAT32_ARRAY,
		"snares": TYPE_PACKED_FLOAT32_ARRAY, "hats": TYPE_PACKED_FLOAT32_ARRAY,
		"kick_amp": TYPE_PACKED_BYTE_ARRAY, "snare_amp": TYPE_PACKED_BYTE_ARRAY,
		"hat_amp": TYPE_PACKED_BYTE_ARRAY, "character": TYPE_INT, "kick_cover": TYPE_FLOAT,
		"four_floor": TYPE_FLOAT, "drop_step": TYPE_FLOAT, "drop_strength": TYPE_PACKED_FLOAT32_ARRAY,
		"levels": TYPE_PACKED_BYTE_ARRAY, "bins": TYPE_INT, "bin_s": TYPE_FLOAT, "analysis": TYPE_INT}
	var bad := []
	for k in types:
		if typeof(m.get(k)) != types[k]:
			bad.append(k)
	_check(bad.is_empty(), "the map has every to_dict field with its type (wrong: %s)" % [bad])
	if not bad.is_empty():
		return
	# Why: the beat grid is what the scenes lock to; a steady 120 BPM track
	# must give 120 BPM and one beat per 0.5 s.
	_check(absf(m.bpm - BPM) < 1.0, "bpm %.2f is near %.0f" % [m.bpm, BPM])
	_check(absf(m.beats.size() - 40.0 / 0.5) <= 4, "about 80 beats (%d)" % m.beats.size())
	_check(m.beat_bar.size() == m.beats.size() and m.sections.size() % 2 == 0, "beat_bar per beat, sections in pairs")
	_check(m.levels.size() == m.bins * 3 and absf(m.bins * m.bin_s - 40.0) < 0.1, "levels: 3 bytes per bin over the song")
	# Why: a bad call from GDScript must not crash the app.
	_check((_call("analyze", [song, 0.0]) as Dictionary).is_empty(), "rate 0: empty")
	_check((_call("analyze", [PackedVector2Array(), 44100.0]) as Dictionary).is_empty(), "no audio: empty")
	_check((_call("analyze_bins", [PackedFloat32Array([1.0]), 5, 0.01, PackedFloat32Array()]) as Dictionary).is_empty(), "bins that do not fit: empty")
	var mono := PackedFloat32Array()
	mono.resize(song.size())
	for i in song.size():
		mono[i] = song[i].x
	var mm: Dictionary = _call("analyze_mono", [mono, float(RATE)])
	# Why: stereo with L == R is the mono signal; both paths must agree.
	_check(mm.get("beats") == m.beats and mm.get("sections") == m.sections, "analyze_mono of the same signal: the same map")

	var r: Dictionary = _call("fingerprint", [song, RATE])
	_check(str(r.get("fingerprint_version", "")).begins_with("chromaprint-rs-0.3.0/") and str(r.get("fingerprint", "")) != "", "fingerprint(): a record (%s)" % r.get("fingerprint_version", ""))
	var lead: Dictionary = _call("fingerprint", [_song(40.0, 0.5, 1, 220.0), RATE])
	var same: Dictionary = _call("matches", [lead.fingerprint, lead.duration, r.fingerprint, r.duration])
	# Why: one recording with other padding keeps its identity, and the
	# offset says how far to shift labels between the copies.
	_check(same.matched and absf(same.offset - 0.5) < 0.15, "0.5 s lead-in: matches, offset %.3f s" % same.get("offset", 0.0))
	var other: Dictionary = _call("fingerprint", [_song(40.0, 0.0, 9, 311.0), RATE])
	var diff: Dictionary = _call("matches", [r.fingerprint, r.duration, other.fingerprint, other.duration])
	# Why: a false match moves one song's labels onto another.
	_check(not diff.matched, "another chord and noise: no match (cover %.2f)" % diff.get("cover", 0.0))
	_check(not (_call("matches", ["not base64!", 1.0, r.fingerprint, r.duration]) as Dictionary).matched, "a broken fingerprint: no match")
