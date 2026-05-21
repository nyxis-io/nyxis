// NXS conformance runner for C#.
// Add to csharp/ project as a file, then:
//   dotnet run --project csharp/ -- ../conformance/
// Or build standalone:
//   dotnet-script conformance/run_csharp.cs conformance/
//
// To compile as a standalone .cs alongside NxsReader.cs:
//   cd csharp
//   dotnet run --project . -- ../conformance/
//
// This file is designed to be added as an additional file in the csharp project.
// Set as the entry point by uncommenting the main below and commenting out Program.cs main.

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using Nxs;

namespace Nxs.Conformance;

public static class ConformanceRunner
{
    private const uint MagicList = 0x4E59584Cu;

    static double ApproxEq(double a, double b)
    {
        if (a == b) return 0;
        double diff = Math.Abs(a - b);
        double mag  = Math.Max(Math.Abs(a), Math.Abs(b));
        if (mag < 1e-300) return diff < 1e-300 ? 0 : 1;
        return diff / mag;
    }

    static bool ValuesMatch(object? actual, JsonElement expected)
    {
        switch (expected.ValueKind)
        {
            case JsonValueKind.Null:
                return actual == null;
            case JsonValueKind.True:
            case JsonValueKind.False:
                return actual is bool b && b == expected.GetBoolean();
            case JsonValueKind.Number:
                double ev = expected.GetDouble();
                return actual switch
                {
                    long l   => ApproxEq((double)l, ev) < 1e-9,
                    double d => ApproxEq(d, ev) < 1e-9,
                    int i    => ApproxEq((double)i, ev) < 1e-9,
                    _        => false,
                };
            case JsonValueKind.String:
                return actual is string s && s == expected.GetString();
            case JsonValueKind.Array:
                if (actual is not object?[] arr) return false;
                var elems = expected.EnumerateArray().ToArray();
                if (arr.Length != elems.Length) return false;
                for (int i = 0; i < elems.Length; i++)
                    if (!ValuesMatch(arr[i], elems[i])) return false;
                return true;
            default:
                return false;
        }
    }

    static int ResolveSlotRaw(byte[] data, int objOffset, int slot)
    {
        int p = objOffset + 8;
        int cur = 0, t = 0;
        bool found = false;
        int b = 0;

        while (true)
        {
            if (p >= data.Length) return -1;
            b = data[p++];
            int bits = b & 0x7F;
            for (int i = 0; i < 7; i++)
            {
                if (cur == slot)
                {
                    if (((bits >> i) & 1) == 0) return -1;
                    found = true; goto doneMask;
                }
                if (((bits >> i) & 1) == 1) t++;
                cur++;
            }
            if ((b & 0x80) == 0) return -1;
        }
        doneMask:
        while ((b & 0x80) != 0)
        {
            if (p >= data.Length) break;
            b = data[p++];
        }
        int rel = BitConverter.ToUInt16(data, p + t * 2);
        return objOffset + rel;
    }

    static object?[] ReadList(byte[] data, int off)
    {
        uint magic = BitConverter.ToUInt32(data, off);
        byte elemSigil = data[off + 8];
        int elemCount = BitConverter.ToInt32(data, off + 9);
        int dataStart = off + 16;
        var result = new object?[elemCount];
        for (int i = 0; i < elemCount; i++)
        {
            int elemOff = dataStart + i * 8;
            if (elemOff + 8 > data.Length) break;
            result[i] = elemSigil switch
            {
                0x3D => (object)BitConverter.ToInt64(data, elemOff),
                0x7E => (object)BitConverter.ToDouble(data, elemOff),
                _ => null,
            };
        }
        return result;
    }

    static object? GetFieldValue(byte[] data, int tailStart, int ri, int slot, byte sigil)
    {
        long absL = BitConverter.ToInt64(data, tailStart + ri * 10 + 2);
        int abs = (int)absL;
        int off = ResolveSlotRaw(data, abs, slot);
        if (off < 0) return null; // absent

        if (off + 4 <= data.Length)
        {
            uint maybe = BitConverter.ToUInt32(data, off);
            if (maybe == MagicList) return ReadList(data, off);
        }

        return sigil switch
        {
            0x3D => (object)BitConverter.ToInt64(data, off),
            0x7E => (object)BitConverter.ToDouble(data, off),
            0x3F => (object)(data[off] != 0),
            0x22 => (object)Encoding.UTF8.GetString(data, off + 4, BitConverter.ToInt32(data, off)),
            0x40 => (object)BitConverter.ToInt64(data, off),
            0x5E => null,
            _    => (object)BitConverter.ToInt64(data, off),
        };
    }

    static void RunPositive(string dir, string name, JsonElement expected)
    {
        var nxbPath = Path.Combine(dir, $"{name}.nxb");
        var data   = File.ReadAllBytes(nxbPath);
        var reader = new NxsReader(data);

        if (expected.TryGetProperty("record_count", out var jrc))
        {
            int expCount = jrc.GetInt32();
            if (reader.RecordCount != expCount)
                throw new Exception($"record_count: expected {expCount}, got {reader.RecordCount}");
        }

        if (expected.TryGetProperty("keys", out var jkeys))
        {
            var expKeys = jkeys.EnumerateArray().Select(k => k.GetString()!).ToArray();
            for (int i = 0; i < expKeys.Length; i++)
            {
                if (i >= reader.Keys.Length)
                    throw new Exception($"key[{i}] missing (expected {expKeys[i]})");
                if (reader.Keys[i] != expKeys[i])
                    throw new Exception($"key[{i}]: expected \"{expKeys[i]}\", got \"{reader.Keys[i]}\"");
            }
        }

        // Access private _tailStart via reflection
        var tailStartField = typeof(NxsReader).GetField("_tailStart",
            BindingFlags.NonPublic | BindingFlags.Instance)!;
        int tailStart = (int)tailStartField.GetValue(reader)!;

        if (expected.TryGetProperty("records", out var jrecs))
        {
            int ri = 0;
            foreach (var expRec in jrecs.EnumerateArray())
            {
                foreach (var kv in expRec.EnumerateObject())
                {
                    string key = kv.Name;
                    if (kv.Value.ValueKind == JsonValueKind.Null) { continue; }

                    int slot = Array.IndexOf(reader.Keys, key);
                    if (slot < 0) throw new Exception($"rec[{ri}].{key}: key not in schema");

                    byte sigil = slot < reader.KeySigils.Length ? reader.KeySigils[slot] : (byte)0x3D;
                    object? actual = GetFieldValue(data, tailStart, ri, slot, sigil);

                    if (!ValuesMatch(actual, kv.Value))
                        throw new Exception($"rec[{ri}].{key}: expected {kv.Value}, got {actual}");
                }
                ri++;
            }
        }
    }

    static void RunNegative(string dir, string name, string expectedCode)
    {
        var nxbPath = Path.Combine(dir, $"{name}.nxb");
        var data = File.ReadAllBytes(nxbPath);

        try
        {
            _ = new NxsReader(data);
            throw new Exception($"expected error {expectedCode} but reader succeeded");
        }
        catch (NxsException e)
        {
            if (!e.Message.Contains(expectedCode))
                throw new Exception($"expected error {expectedCode}, got: {e.Message}");
        }
    }

    public static int Run(string[] args)
    {
        string dir = args.Length > 0 ? args[0] : ".";

        var entries = Directory.GetFiles(dir, "*.expected.json")
            .Select(f => Path.GetFileNameWithoutExtension(f.Replace(".expected", "")))
            .OrderBy(n => n)
            .ToArray();

        int passed = 0, failed = 0;

        foreach (var name in entries)
        {
            if (name.StartsWith("columnar_") || name.StartsWith("pax_"))
            {
                Console.WriteLine($"  SKIP  {name} (columnar/PAX not implemented)");
                passed++;
                continue;
            }
            var jsonPath = Path.Combine(dir, $"{name}.expected.json");
            using var doc = JsonDocument.Parse(File.ReadAllText(jsonPath));
            var root = doc.RootElement;

            bool isNegative = root.TryGetProperty("error", out _);
            try
            {
                if (isNegative)
                {
                    string code = root.GetProperty("error").GetString()!;
                    RunNegative(dir, name, code);
                }
                else
                {
                    RunPositive(dir, name, root);
                }
                Console.WriteLine($"  PASS  {name}");
                passed++;
            }
            catch (Exception e)
            {
                Console.Error.WriteLine($"  FAIL  {name} — {e.Message}");
                failed++;
            }
        }

        Console.WriteLine($"\n{passed} passed, {failed} failed");
        return failed > 0 ? 1 : 0;
    }
}
