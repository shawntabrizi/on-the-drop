extends Node
## Headless check that the on-the-drop GDExtension loads and both calls work.
## Prints "otd-spike: PASS" or "otd-spike: FAIL ..." and quits with 0 or 1.
## Calls go through ClassDB (not the OnTheDrop identifier) so this script
## still parses, and the app still exits by itself, when the library is missing.

const EXPECTED_VERSION := "0.1.0"
const RATE := 48000
const HOP := 1024

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


func _sine(hz: float, n: int) -> PackedFloat32Array:
	var a := PackedFloat32Array()
	a.resize(n)
	for i in n:
		a[i] = sin(TAU * hz * i / RATE)
	return a


func _mean_after_first(a: PackedFloat32Array) -> float:
	var s := 0.0
	for i in range(1, a.size()):
		s += a[i]
	return s / maxf(1.0, a.size() - 1)


func _run() -> void:
	_check(ClassDB.class_exists("OnTheDrop"), "class OnTheDrop is registered")
	if not ClassDB.class_exists("OnTheDrop"):
		return
	var v: String = ClassDB.class_call_static("OnTheDrop", "version")
	print("otd-spike:   version() = %s" % v)
	# Why: the Godot side must load the core it was built with.
	_check(v == EXPECTED_VERSION, "version() == %s" % EXPECTED_VERSION)

	var n := RATE  # 1 s
	var bass: PackedFloat32Array = ClassDB.class_call_static("OnTheDrop", "band_energy", _sine(50.0, n), RATE, HOP)
	var treble: PackedFloat32Array = ClassDB.class_call_static("OnTheDrop", "band_energy", _sine(5000.0, n), RATE, HOP)
	# Why: one value per full hop, so index * HOP / RATE maps to time.
	_check(bass.size() == n / HOP, "band_energy size == %d (got %d)" % [n / HOP, bass.size()])
	var b := _mean_after_first(bass)
	var t := _mean_after_first(treble)
	print("otd-spike:   bass energy %.4f, treble energy %.6f" % [b, t])
	# Why: bass drives the large forms; a 5 kHz hi-hat must not.
	_check(b > 0.3, "50 Hz keeps its energy")
	_check(t < b / 100.0, "5 kHz is >= 20 dB below 50 Hz")
	var empty: PackedFloat32Array = ClassDB.class_call_static("OnTheDrop", "band_energy", _sine(50.0, 100), RATE, 0)
	# Why: a bad hop from GDScript must not crash the app.
	_check(empty.is_empty(), "hop 0 gives an empty result")
