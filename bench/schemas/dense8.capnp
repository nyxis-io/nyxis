@0xb7e6d5c4a3928170;

struct Dense8Record {
  id       @0 :Int64;
  bucket   @1 :Int64;
  quantity @2 :Int64;
  amount   @3 :Float64;
  rate     @4 :Float64;
  score    @5 :Float64;
  category @6 :Int64;
  active   @7 :Bool;
}

struct Dense8File {
  records @0 :List(Dense8Record);
}
