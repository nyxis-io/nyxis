// NXS conformance runner for Kotlin.
// Compile: cd kotlin && ./gradlew assemble && kotlinc -cp build/libs/nxs-1.0.jar ../conformance/run_kotlin.kt -include-runtime -d /tmp/run_kotlin.jar
// Run:     java -jar /tmp/run_kotlin.jar conformance/
//
// Or for a quick single-file run (standalone, includes NxsReader inline):
//   kotlinc -script conformance/run_kotlin.kts conformance/

package conformance

import nxs.NxsReader
import nxs.NxsError
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.abs
import kotlin.math.max

fun approxEq(a: Double, b: Double): Boolean {
    if (a == b) return true
    val diff = abs(a - b)
    val mag = max(abs(a), abs(b))
    if (mag < 1e-300) return diff < 1e-300
    return diff / mag < 1e-9
}

fun valuesMatch(actual: Any?, expected: Any?): Boolean {
    if (expected == null) return actual == null || actual == 0L || actual == 0.0 || actual == false
    if (expected is Boolean) return actual == expected
    if (expected is Double) {
        return when (actual) {
            is Double -> approxEq(actual, expected)
            is Long   -> approxEq(actual.toDouble(), expected)
            is Int    -> approxEq(actual.toDouble(), expected)
            else      -> false
        }
    }
    if (expected is Long || expected is Int) {
        val ev = (expected as Number).toLong()
        return when (actual) {
            is Long   -> actual == ev
            is Int    -> actual.toLong() == ev
            is Double -> approxEq(actual, ev.toDouble())
            else      -> false
        }
    }
    if (expected is String) return actual == expected
    if (expected is List<*>) {
        val ae = actual as? List<*> ?: return false
        if (ae.size != expected.size) return false
        return expected.zip(ae).all { (e, a) -> valuesMatch(a, e) }
    }
    return false
}

const val MAGIC_LIST: Int = 0x4E59584C.toInt()

fun readList(data: ByteArray, off: Int): List<Any>? {
    if (off + 16 > data.size) return null
    val buf = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
    val magic = buf.getInt(off)
    if (magic != MAGIC_LIST) return null
    val elemSigil = data[off + 8].toInt() and 0xFF
    val elemCount = buf.getInt(off + 9)
    val dataStart = off + 16
    val result = mutableListOf<Any>()
    for (i in 0 until elemCount) {
        val elemOff = dataStart + i * 8
        if (elemOff + 8 > data.size) break
        when (elemSigil) {
            0x3D -> result.add(buf.getLong(elemOff))    // = int
            0x7E -> result.add(buf.getDouble(elemOff))  // ~ float
            else -> result.add(0L)
        }
    }
    return result
}

fun resolveSlotRaw(data: ByteArray, objOffset: Int, slot: Int): Int {
    var p = objOffset + 8
    var cur = 0
    var t = 0
    var found = false
    var b = 0

    while (true) {
        if (p >= data.size) return -1
        b = data[p++].toInt() and 0xFF
        val bits = b and 0x7F
        for (i in 0 until 7) {
            if (cur == slot) {
                if ((bits shr i) and 1 == 0) return -1
                found = true
                break
            }
            if ((bits shr i) and 1 == 1) t++
            cur++
        }
        if (found) break
        if (b and 0x80 == 0) return -1
    }

    while (b and 0x80 != 0) {
        if (p >= data.size) break
        b = data[p++].toInt() and 0xFF
    }

    val buf = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
    val rel = buf.getShort(p + t * 2).toInt() and 0xFFFF
    return objOffset + rel
}

fun getFieldValue(data: ByteArray, tailStart: Int, ri: Int, slot: Int, sigil: Byte): Any? {
    val buf = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
    val abs = buf.getLong(tailStart + ri * 10 + 2).toInt()
    val off = resolveSlotRaw(data, abs, slot)
    if (off < 0) return Unit  // absent sentinel

    if (off + 4 <= data.size) {
        val maybe = buf.getInt(off)
        if (maybe == MAGIC_LIST) return readList(data, off)
    }

    return when (sigil.toInt() and 0xFF) {
        0x3D -> buf.getLong(off)            // = int
        0x7E -> buf.getDouble(off)          // ~ float
        0x3F -> (data[off].toInt() and 0xFF) != 0  // ? bool
        0x22 -> {                            // " str
            val len = buf.getInt(off)
            String(data, off + 4, len, Charsets.UTF_8)
        }
        0x40 -> buf.getLong(off)            // @ time
        0x5E -> null                         // ^ null
        else -> buf.getLong(off)
    }
}

@Throws(Exception::class)
fun runPositive(dir: String, name: String, expected: Map<String, Any>) {
    val nxbPath = "$dir/$name.nxb"
    val data = File(nxbPath).readBytes()
    val reader = NxsReader(data)

    val expCount = (expected["record_count"] as? Number)?.toInt()
    if (expCount != null && reader.recordCount != expCount) {
        throw Exception("record_count: expected $expCount, got ${reader.recordCount}")
    }

    @Suppress("UNCHECKED_CAST")
    val expKeys = expected["keys"] as? List<String> ?: emptyList()
    for ((i, expKey) in expKeys.withIndex()) {
        if (i >= reader.keys.size) throw Exception("key[$i] missing (expected $expKey)")
        if (reader.keys[i] != expKey) throw Exception("key[$i]: expected \"$expKey\", got \"${reader.keys[i]}\"")
    }

    // Access internal tailStart via reflection
    val tailStartField = reader.javaClass.getDeclaredField("tailStart")
    tailStartField.isAccessible = true
    val tailStart = tailStartField.getInt(reader)

    @Suppress("UNCHECKED_CAST")
    val expRecords = expected["records"] as? List<Map<String, Any>> ?: emptyList()
    for ((ri, expRec) in expRecords.withIndex()) {
        for ((key, expVal) in expRec) {
            val slot = reader.keys.indexOf(key)
            if (slot < 0) throw Exception("rec[$ri].$key: key not in schema")
            val sigil = if (slot < reader.keySigils.size) reader.keySigils[slot] else 0x3D.toByte()

            if (expVal == null) continue  // null — skip

            val actual = getFieldValue(data, tailStart, ri, slot, sigil)
            if (actual === Unit) throw Exception("rec[$ri].$key: field absent (expected $expVal)")
            if (!valuesMatch(actual, expVal)) {
                throw Exception("rec[$ri].$key: expected $expVal (${expVal::class}), got $actual (${actual?.javaClass})")
            }
        }
    }
}

fun runNegative(dir: String, name: String, expectedCode: String) {
    val nxbPath = "$dir/$name.nxb"
    val data = File(nxbPath).readBytes()
    try {
        NxsReader(data)
        throw Exception("expected error $expectedCode but reader succeeded")
    } catch (e: NxsError) {
        val msg = e.message ?: ""
        if (!msg.contains(expectedCode)) {
            throw Exception("expected error $expectedCode, got: $msg")
        }
    }
}

@Suppress("UNCHECKED_CAST")
fun parseJsonObj(json: String): Map<String, Any>? {
    // Use Kotlin's built-in JSON via a simple approach
    // Since we're on JVM, use org.json or manual parsing
    // For portability, we do a simple manual parse
    return try {
        val obj = mutableMapOf<String, Any>()
        // Try to use Jackson or Gson if available, else fallback
        // For Gradle-built jar, we rely on Jackson (included as test dep)
        // Actually, let's use a basic approach:
        val cls = Class.forName("com.fasterxml.jackson.databind.ObjectMapper")
        val mapper = cls.getDeclaredConstructor().newInstance()
        val readValue = cls.getMethod("readValue", String::class.java, Class::class.java)
        readValue.invoke(mapper, json, Map::class.java) as? Map<String, Any>
    } catch (e: Exception) {
        // Fallback: parse manually (limited)
        null
    }
}

fun main(args: Array<String>) {
    val dir = if (args.isNotEmpty()) args[0] else "."

    val entries = File(dir).listFiles { f -> f.name.endsWith(".expected.json") }
        ?.map { it.name.removeSuffix(".expected.json") }
        ?.sorted()
        ?: emptyList()

    var passed = 0; var failed = 0

    for (name in entries) {
        val jsonFile = File("$dir/$name.expected.json")
        val jsonStr  = jsonFile.readText()

        // Parse JSON using available library
        val expected = try {
            val mapper = Class.forName("com.fasterxml.jackson.databind.ObjectMapper")
                .getDeclaredConstructor().newInstance()
            @Suppress("UNCHECKED_CAST")
            Class.forName("com.fasterxml.jackson.databind.ObjectMapper")
                .getMethod("readValue", String::class.java, Class::class.java)
                .invoke(mapper, jsonStr, Map::class.java) as Map<String, Any>
        } catch (e: Exception) {
            // Try org.json
            try {
                val jo = Class.forName("org.json.JSONObject")
                    .getDeclaredConstructor(String::class.java)
                    .newInstance(jsonStr)
                // Convert to map - simplified
                @Suppress("UNCHECKED_CAST")
                jo.javaClass.getMethod("toMap").invoke(jo) as Map<String, Any>
            } catch (e2: Exception) {
                System.err.println("  SKIP  $name — no JSON library available")
                continue
            }
        }

        val isNegative = expected.containsKey("error")
        try {
            if (isNegative) {
                runNegative(dir, name, expected["error"] as String)
            } else {
                runPositive(dir, name, expected)
            }
            println("  PASS  $name")
            passed++
        } catch (e: Exception) {
            System.err.println("  FAIL  $name — ${e.message}")
            failed++
        }
    }

    println("\n$passed passed, $failed failed")
    if (failed > 0) System.exit(1)
}
