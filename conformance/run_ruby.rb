#!/usr/bin/env ruby
# frozen_string_literal: true
# NXS conformance runner for Ruby.
# Usage: ruby conformance/run_ruby.rb conformance/

require "json"

drv = ENV["DRV"] || File.expand_path("../../nyxis-drivers", __dir__)
require File.join(drv, "ruby", "nxs")

MAGIC_LIST = 0x4E59584C

def approx_eq(a, b)
  return true if a == b
  return false unless a.is_a?(Numeric) && b.is_a?(Numeric)
  diff = (a.to_f - b.to_f).abs
  mag  = [a.to_f.abs, b.to_f.abs].max
  return diff < 1e-300 if mag < 1e-300
  diff / mag < 1e-9
end

def read_list(data, off)
  return nil if off + 16 > data.bytesize
  magic = data.unpack1("@#{off}L<")
  return nil unless magic == MAGIC_LIST
  elem_sigil = data.getbyte(off + 8)
  elem_count = data.unpack1("@#{off + 9}L<")
  data_start = off + 16
  result = []
  elem_count.times do |i|
    elem_off = data_start + i * 8
    break if elem_off + 8 > data.bytesize
    case elem_sigil
    when 0x3D  # = int
      result << data.unpack1("@#{elem_off}q<")
    when 0x7E  # ~ float
      result << data.unpack1("@#{elem_off}E")
    else
      result << nil
    end
  end
  result
end

# Raw bitmask walk to resolve slot offset from raw data
def resolve_slot_raw(data, obj_offset, slot)
  p   = obj_offset + 8  # skip magic + length
  cur = 0
  t   = 0
  found = false
  b = 0

  loop do
    return nil if p >= data.bytesize
    b    = data.getbyte(p); p += 1
    bits = b & 0x7F
    7.times do |i|
      if cur == slot
        return nil if (bits >> i) & 1 == 0
        found = true
        break
      end
      t += 1 if (bits >> i) & 1 == 1
      cur += 1
    end
    break if found
    return nil if (b & 0x80) == 0
  end

  # drain remaining continuation bytes
  while (b & 0x80) != 0
    return nil if p >= data.bytesize
    b = data.getbyte(p); p += 1
  end

  rel = data.unpack1("@#{p + t * 2}S<")
  obj_offset + rel
end

def get_field_value(data, reader, tail_start, ri, slot, sigil_byte)
  # Get abs offset from tail index
  abs = data.unpack1("@#{tail_start + ri * 10 + 2}Q<")
  off = resolve_slot_raw(data, abs, slot)
  return :absent if off.nil?

  # Check for list magic
  if off + 4 <= data.bytesize
    maybe_magic = data.unpack1("@#{off}L<")
    if maybe_magic == MAGIC_LIST
      return read_list(data, off)
    end
  end

  case sigil_byte
  when 0x3D  # = int
    data.unpack1("@#{off}q<")
  when 0x7E  # ~ float
    data.unpack1("@#{off}E")
  when 0x3F  # ? bool
    data.getbyte(off) != 0
  when 0x22  # " str
    len = data.unpack1("@#{off}L<")
    data[off + 4, len].force_encoding("UTF-8")
  when 0x40  # @ time
    data.unpack1("@#{off}q<")
  when 0x5E  # ^ null
    nil
  else
    # try i64
    data.unpack1("@#{off}q<") rescue nil
  end
end

def values_match(actual, expected)
  return true  if expected.nil? && (actual.nil? || actual == 0 || actual == false)
  return false if expected.nil?
  return expected == actual if expected.is_a?(TrueClass) || expected.is_a?(FalseClass)
  if expected.is_a?(Numeric)
    return false unless actual.is_a?(Numeric)
    return approx_eq(actual.to_f, expected.to_f)
  end
  return expected == actual if expected.is_a?(String)
  if expected.is_a?(Array)
    return false unless actual.is_a?(Array) && actual.length == expected.length
    expected.each_with_index { |e, i| return false unless values_match(actual[i], e) }
    return true
  end
  false
end

def run_positive(conformance_dir, name, expected)
  nxb_path = File.join(conformance_dir, "#{name}.nxb")
  data = File.binread(nxb_path)
  reader = Nxs::Reader.new(data)

  if reader.record_count != expected["record_count"]
    raise "record_count: expected #{expected['record_count']}, got #{reader.record_count}"
  end

  expected["keys"].each_with_index do |exp_key, i|
    actual_key = reader.keys[i]
    unless actual_key == exp_key
      raise "key[#{i}]: expected #{exp_key.inspect}, got #{actual_key.inspect}"
    end
  end

  tail_start = reader.instance_variable_get(:@tail_start)
  sigils     = reader.instance_variable_get(:@key_sigils)
  key_index  = reader.key_index

  expected["records"].each_with_index do |exp_rec, ri|
    exp_rec.each do |key, exp_val|
      slot = key_index[key]
      raise "rec[#{ri}].#{key}: key not in schema" if slot.nil?

      sigil = sigils[slot] || 0x3D
      actual = get_field_value(data, reader, tail_start, ri, slot, sigil)

      if exp_val.nil?
        # null — accept nil or absent
        next
      end
      if actual == :absent
        raise "rec[#{ri}].#{key}: field absent, expected #{exp_val.inspect}"
      end
      unless values_match(actual, exp_val)
        raise "rec[#{ri}].#{key}: expected #{exp_val.inspect}, got #{actual.inspect}"
      end
    end
  end
end

def run_negative(conformance_dir, name, expected_code)
  nxb_path = File.join(conformance_dir, "#{name}.nxb")
  data = File.binread(nxb_path)
  begin
    Nxs::Reader.new(data)
    raise "expected error #{expected_code.inspect} but reader succeeded"
  rescue Nxs::NxsError => e
    code = e.code
    unless code == expected_code
      raise "expected error #{expected_code.inspect}, got #{code.inspect} (#{e.message})"
    end
  end
end

# ── Main ──────────────────────────────────────────────────────────────────────

conformance_dir = ARGV[0] || File.dirname(__FILE__)

entries = Dir[File.join(conformance_dir, "*.expected.json")]
  .map { |f| File.basename(f, ".expected.json") }
  .sort

passed = 0
failed = 0

entries.each do |name|
  json_path = File.join(conformance_dir, "#{name}.expected.json")
  expected  = JSON.parse(File.read(json_path))
  is_negative = expected.key?("error")

  begin
    if is_negative
      run_negative(conformance_dir, name, expected["error"])
    else
      run_positive(conformance_dir, name, expected)
    end
    puts "  PASS  #{name}"
    passed += 1
  rescue => e
    $stderr.puts "  FAIL  #{name} — #{e.message}"
    failed += 1
  end
end

puts "\n#{passed} passed, #{failed} failed"
exit(failed > 0 ? 1 : 0)
