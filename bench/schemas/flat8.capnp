@0x8f3c2a1b9e7d4c50;

struct Flat8Record {
  id         @0  :Int64;
  username   @1  :Text;
  email      @2  :Text;
  age        @3  :Int64;
  balance    @4  :Float64;
  active     @5  :Bool;
  score      @6  :Float64;
  createdAt  @7  :Int64;
}

struct Flat8File {
  records @0 :List(Flat8Record);
}
