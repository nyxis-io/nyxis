@0xa1b2c3d4e5f60718;

struct SparseGrandchild {
  gcI64 @0 :Int64;
  gcStr @1 :Text;
}

struct SparseChild {
  grandchild @0 :SparseGrandchild;
  childF64   @1 :Float64;
}

struct SparseMeta {
  child     @0 :SparseChild;
  metaFlag  @1 :Bool;
}

struct SparseRecord {
  i01 @0  :Int64;  i02 @1  :Int64;  i03 @2  :Int64;  i04 @3  :Int64;  i05 @4  :Int64;
  i06 @5  :Int64;  i07 @6  :Int64;  i08 @7  :Int64;  i09 @8  :Int64;  i10 @9  :Int64;
  i11 @10 :Int64;  i12 @11 :Int64;  i13 @12 :Int64;  i14 @13 :Int64;  i15 @14 :Int64;
  i16 @15 :Int64;  i17 @16 :Int64;  i18 @17 :Int64;  i19 @18 :Int64;  i20 @19 :Int64;
  s21 @20 :Text;   s22 @21 :Text;   s23 @22 :Text;   s24 @23 :Text;   s25 @24 :Text;
  s26 @25 :Text;   s27 @26 :Text;   s28 @27 :Text;   s29 @28 :Text;   s30 @29 :Text;
  s31 @30 :Text;   s32 @31 :Text;   s33 @32 :Text;   s34 @33 :Text;   s35 @34 :Text;
  f36 @35 :Float64; f37 @36 :Float64; f38 @37 :Float64; f39 @38 :Float64; f40 @39 :Float64;
  f41 @40 :Float64; f42 @41 :Float64; f43 @42 :Float64; f44 @43 :Float64; f45 @44 :Float64;
  b46 @45 :Bool; b47 @46 :Bool; b48 @47 :Bool; b49 @48 :Bool; b50 @49 :Bool;
  meta @50 :SparseMeta;
}

struct SparseFile {
  records @0 :List(SparseRecord);
}
