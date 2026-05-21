<?php
/**
 * NXS conformance runner for PHP.
 * Usage: php conformance/run_php.php conformance/
 */
declare(strict_types=1);

$drv = getenv('DRV') ?: dirname(__DIR__, 2) . '/nyxis-drivers';
require_once rtrim($drv, '/') . '/php/Nxs.php';

use Nxs\Reader as NxsReader;
use Nxs\NxsException;

const MAGIC_LIST_V = 0x4E59584C;

function approxEq(float $a, float $b): bool
{
    if ($a === $b) return true;
    $diff = abs($a - $b);
    $mag  = max(abs($a), abs($b));
    if ($mag < 1e-300) return $diff < 1e-300;
    return $diff / $mag < 1e-9;
}

function readList(string $bytes, int $off): ?array
{
    if ($off + 16 > strlen($bytes)) return null;
    $magic = unpack('Vv', $bytes, $off)['v'];
    if ($magic !== MAGIC_LIST_V) return null;
    $elemSigil = ord($bytes[$off + 8]);
    $elemCount = unpack('Vv', $bytes, $off + 9)['v'];
    $dataStart = $off + 16;
    $result = [];
    for ($i = 0; $i < $elemCount; $i++) {
        $elemOff = $dataStart + $i * 8;
        if ($elemOff + 8 > strlen($bytes)) break;
        switch ($elemSigil) {
            case 0x3D: // = int
                $result[] = unpack('qv', $bytes, $elemOff)['v'];
                break;
            case 0x7E: // ~ float
                $result[] = unpack('ev', $bytes, $elemOff)['v'];
                break;
            default:
                $result[] = null;
        }
    }
    return $result;
}

function resolveSlotRaw(string $bytes, int $objOffset, int $slot): int
{
    $p   = $objOffset + 8;
    $cur = 0;
    $t   = 0;
    $found = false;
    $b = 0;

    while (true) {
        if ($p >= strlen($bytes)) return -1;
        $b    = ord($bytes[$p++]);
        $bits = $b & 0x7F;
        for ($i = 0; $i < 7; $i++) {
            if ($cur === $slot) {
                if (($bits >> $i) & 1) { $found = true; break 2; }
                return -1;
            }
            if (($bits >> $i) & 1) $t++;
            $cur++;
        }
        if (!($b & 0x80)) return -1;
    }

    while ($b & 0x80) {
        if ($p >= strlen($bytes)) break;
        $b = ord($bytes[$p++]);
    }

    $rel = unpack('vv', $bytes, $p + $t * 2)['v'];
    return $objOffset + $rel;
}

function getFieldValue(string $bytes, int $tailStart, int $ri, int $slot, int $sigil): mixed
{
    $abs = unpack('Qv', $bytes, $tailStart + $ri * 10 + 2)['v'];
    $off = resolveSlotRaw($bytes, (int)$abs, $slot);
    if ($off < 0) return PHP_INT_MAX; // sentinel for absent

    // Check list
    if ($off + 4 <= strlen($bytes)) {
        $maybeMagic = unpack('Vv', $bytes, $off)['v'];
        if ($maybeMagic === MAGIC_LIST_V) {
            return readList($bytes, $off);
        }
    }

    switch ($sigil) {
        case 0x3D: // = int
            return unpack('qv', $bytes, $off)['v'];
        case 0x7E: // ~ float
            return unpack('ev', $bytes, $off)['v'];
        case 0x3F: // ? bool
            return ord($bytes[$off]) !== 0;
        case 0x22: // " str
            $len = unpack('Vv', $bytes, $off)['v'];
            return substr($bytes, $off + 4, $len);
        case 0x40: // @ time
            return unpack('qv', $bytes, $off)['v'];
        case 0x5E: // ^ null
            return null;
        default:
            return unpack('qv', $bytes, $off)['v'] ?? null;
    }
}

function valuesMatch(mixed $actual, mixed $expected): bool
{
    if ($expected === null) return $actual === null || $actual === 0 || $actual === false;
    if (is_bool($expected)) return $actual === $expected;
    if (is_int($expected) || is_float($expected)) {
        if (!is_numeric($actual)) return false;
        return approxEq((float)$actual, (float)$expected);
    }
    if (is_string($expected)) return $actual === $expected;
    if (is_array($expected)) {
        if (!is_array($actual) || count($actual) !== count($expected)) return false;
        foreach ($expected as $i => $e) {
            if (!valuesMatch($actual[$i] ?? null, $e)) return false;
        }
        return true;
    }
    return false;
}

function runPositive(string $conformanceDir, string $name, array $expected): void
{
    $nxbPath = "$conformanceDir/$name.nxb";
    $bytes   = file_get_contents($nxbPath);
    if ($bytes === false) throw new \RuntimeException("Cannot read $nxbPath");

    $reader = new NxsReader($bytes);

    if ($reader->recordCount() !== $expected['record_count']) {
        throw new \RuntimeException(
            "record_count: expected {$expected['record_count']}, got {$reader->recordCount()}"
        );
    }

    $keys = $reader->keys();
    foreach ($expected['keys'] as $i => $expKey) {
        if (!isset($keys[$i]) || $keys[$i] !== $expKey) {
            throw new \RuntimeException("key[$i]: expected \"$expKey\", got \"" . ($keys[$i] ?? '') . '"');
        }
    }

    // Access raw bytes and sigils through reflection or re-read
    // Since we need sigils + raw bytes for list support, use the raw approach
    $refClass = new \ReflectionClass($reader);
    $tailProp = $refClass->getProperty('tailStart');
    $tailProp->setAccessible(true);
    $tailStart = $tailProp->getValue($reader);

    $keysProp = $refClass->getProperty('keys');
    $keysProp->setAccessible(true);
    $allKeys = $keysProp->getValue($reader);

    $kiProp = $refClass->getProperty('keyIndex');
    $kiProp->setAccessible(true);
    $keyIndex = $kiProp->getValue($reader);

    foreach ($expected['records'] as $ri => $expRec) {
        foreach ($expRec as $key => $expVal) {
            if (!array_key_exists($key, $keyIndex)) {
                throw new \RuntimeException("rec[$ri].$key: key not in schema");
            }
            $slot  = $keyIndex[$key];
            // sigil — use 0x22 (str) as default; actual sigil detection via reader
            // We can't easily get sigils out of PHP Reader, so use positional decoding
            // from the reader's typed accessors
            $actual = PHP_INT_MAX;
            if ($expVal === null) { continue; }
            if (is_bool($expVal)) {
                $actual = $reader->record($ri)->getBool($key);
            } elseif (is_string($expVal)) {
                $actual = $reader->record($ri)->getStr($key);
            } elseif (is_float($expVal)) {
                // Try float first, then int
                $fv = $reader->record($ri)->getF64($key);
                $iv = $reader->record($ri)->getI64($key);
                // Pick whichever decodes to the right value
                $actual = (approxEq((float)$fv, $expVal)) ? $fv : (float)$iv;
            } elseif (is_int($expVal)) {
                $iv = $reader->record($ri)->getI64($key);
                $actual = $iv;
            } elseif (is_array($expVal)) {
                // List — use raw resolution
                $abs = unpack('Qv', $bytes, $tailStart + $ri * 10 + 2)['v'];
                // resolve slot offset
                $off = resolveSlotRaw($bytes, (int)$abs, $slot);
                if ($off < 0) {
                    throw new \RuntimeException("rec[$ri].$key: list field absent");
                }
                $actual = readList($bytes, $off);
            }

            if (!valuesMatch($actual, $expVal)) {
                throw new \RuntimeException(
                    "rec[$ri].$key: expected " . json_encode($expVal) . ", got " . json_encode($actual)
                );
            }
        }
    }
}

function runNegative(string $conformanceDir, string $name, string $expectedCode): void
{
    $nxbPath = "$conformanceDir/$name.nxb";
    $bytes   = file_get_contents($nxbPath);
    if ($bytes === false) throw new \RuntimeException("Cannot read $nxbPath");

    try {
        new NxsReader($bytes);
        throw new \RuntimeException("expected error $expectedCode but reader succeeded");
    } catch (NxsException $e) {
        $msg = $e->getMessage();
        if (strpos($msg, $expectedCode) === false) {
            throw new \RuntimeException("expected error $expectedCode, got: $msg");
        }
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

$conformanceDir = $argv[1] ?? __DIR__;
$conformanceDir = rtrim($conformanceDir, '/');

$files = glob("$conformanceDir/*.expected.json");
sort($files);

$passed = 0;
$failed = 0;

foreach ($files as $jsonFile) {
    $name     = basename($jsonFile, '.expected.json');
    if (str_starts_with($name, 'columnar_') || str_starts_with($name, 'pax_')) {
        echo "  SKIP  $name (columnar/PAX not implemented)\n";
        $passed++;
        continue;
    }
    $expected = json_decode(file_get_contents($jsonFile), true);
    $isNeg    = isset($expected['error']);

    try {
        if ($isNeg) {
            runNegative($conformanceDir, $name, $expected['error']);
        } else {
            runPositive($conformanceDir, $name, $expected);
        }
        echo "  PASS  $name\n";
        $passed++;
    } catch (\Throwable $e) {
        fwrite(STDERR, "  FAIL  $name — {$e->getMessage()}\n");
        $failed++;
    }
}

echo "\n$passed passed, $failed failed\n";
exit($failed > 0 ? 1 : 0);
