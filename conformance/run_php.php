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
const SIGIL_LIST   = 0x4C;

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

function getFieldValue(Nxs\NxsObject $obj, NxsReader $reader, string $key): mixed
{
    $slot = $reader->slotOf($key);
    if ($slot < 0) {
        return PHP_INT_MAX;
    }

    $sigils = $reader->sigils();
    $sigil  = $sigils[$slot] ?? 0;

    if ($sigil === SIGIL_LIST) {
        if ($reader->layout() !== 'row') {
            return PHP_INT_MAX;
        }
        $bytes = $reader->rawBytes();
        $ref   = new ReflectionClass($obj);
        $offProp = $ref->getProperty('offset');
        $offProp->setAccessible(true);
        $objOff = $offProp->getValue($obj);
        $fieldOff = resolveSlotRaw($bytes, $objOff, $slot);
        if ($fieldOff < 0) {
            return PHP_INT_MAX;
        }
        return readList($bytes, $fieldOff);
    }

    if ($sigil === ord('"')) {
        return $obj->getStr($key);
    }
    if ($sigil === ord('?')) {
        return $obj->getBool($key);
    }
    if ($sigil === 0x7E) {
        return $obj->getF64($key);
    }
    if ($sigil === ord('=') || $sigil === ord('@')) {
        return $obj->getI64($key);
    }
    if ($sigil === ord('^')) {
        return null;
    }

    return $obj->getI64($key);
}

function valuesMatch(mixed $actual, mixed $expected): bool
{
    if ($expected === null) return $actual === null || $actual === 0 || $actual === false;
    if (is_bool($expected)) return $actual === $expected;
    if (is_int($expected) || is_float($expected)) {
        if (!is_int($actual) && !is_float($actual)) return false;
        return approxEq((float)$actual, (float)$expected);
    }
    if (is_string($expected)) {
        return is_string($actual) && $actual === $expected;
    }
    if (is_array($expected)) {
        if (!is_array($actual)) return false;
        if (count($actual) !== count($expected)) return false;
        foreach ($expected as $i => $exp) {
            if (!valuesMatch($actual[$i] ?? null, $exp)) return false;
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

    foreach ($expected['records'] as $ri => $expRec) {
        $obj = $reader->record($ri);
        foreach ($expRec as $key => $expVal) {
            if (!array_key_exists($key, $expRec)) {
                continue;
            }
            $actual = getFieldValue($obj, $reader, $key);
            if ($expVal === null) {
                if ($actual !== null && $actual !== 0 && $actual !== false && $actual !== '') {
                    throw new \RuntimeException(
                        "rec[$ri].$key: expected null, got " . json_encode($actual)
                    );
                }
                continue;
            }
            if ($actual === PHP_INT_MAX) {
                throw new \RuntimeException("rec[$ri].$key: field absent");
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
        echo "  FAIL  $name: {$e->getMessage()}\n";
        $failed++;
    }
}

echo "\n$passed passed, $failed failed\n";
exit($failed > 0 ? 1 : 0);
