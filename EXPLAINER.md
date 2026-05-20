# What Is NXS?

---

## For non-technical people

Imagine you have a giant spreadsheet with millions of rows — payroll history, customer records, event logs. Every time you open it, you have to wait for the entire file to load before you can see anything.

NXS solves that problem.

It's a **file format** (like PDF or CSV) designed so that programs can jump directly to the exact row they need — without reading every other row first. Opening a file with a million records is nearly instant because the file has a built-in index at the end, like the index at the back of a book.

Two key ideas make it work:

1. **A human-readable source file (`.nxs`)** — people write data in a clean, readable text format that looks a bit like a structured list with labels.
2. **A compiled binary file (`.nxb`)** — a tool (the compiler) converts that text into a compact, machine-optimised file that can be read at full speed by any app.

Once you have the `.nxb` file, ten different programming languages can read it — Rust, JavaScript, Python, Go, Ruby, PHP, C, Swift, Kotlin, and C# — without any translation layer between them. The same file works everywhere.

**What NXS is good for:**
- Large structured datasets that need to be opened quickly
- Tools that look up individual records rather than scanning everything
- Data that gets shared between systems built in different languages
- Browser apps, command-line tools, and internal tooling where loading a full database would be overkill

**What NXS is not:**
- A database (it doesn't handle transactions or queries)
- A replacement for JSON in APIs
- A columnar analytics engine like Parquet or Arrow

---

## For junior engineers

NXS is a **bi-modal serialization format**: one human-authored text source (`.nxs`) that a Rust compiler turns into a zero-copy binary (`.nxb`). Think of it as "write once in readable text, run fast everywhere."

### The two files

```
data.nxs  →  [nxs compiler]  →  data.nxb
  (you write this)               (programs read this)
```

The `.nxs` source uses **sigils** — one-character type prefixes that tell the compiler exactly what kind of value follows:

```
user {
    id:         =1024       # = means Int64
    active:     ?true       # ? means Bool
    score:      ~9.81       # ~ means Float64
    name:       "Alex"      # " means UTF-8 string
    role:       $admin      # $ means dictionary-interned keyword
    created_at: @2026-04-30 # @ means timestamp (Unix nanoseconds)
    deleted_at: ^           # ^ means null (explicitly absent)
}
```

No schema file required separately — the schema is embedded inside the `.nxb` file itself.

### Why it's fast

Three mechanisms make reads fast:

**1. Tail-index (O(1) record lookup)**
The final footer of every sealed `.nxb` file points to an index that maps record positions to their exact byte offsets. To get record #500,000, the reader jumps to that record's 10-byte index entry — it never scans the data.

**2. Offset table (O(1) field access)**
Inside each record there's a small table mapping field names to byte offsets. Getting a single field is one pointer dereference, not a scan through the whole record.

**3. 8-byte alignment (zero-copy for numeric types)**
Integers, floats, and timestamps are stored at 8-byte-aligned positions. A program can memory-map the file and read those values as native CPU types directly — no copy, no parse.

### Key distinction: absent vs. null

NXS distinguishes between a field that is **not present** (doesn't exist in this record) and a field that is **explicitly null** (`^`). Many formats collapse these into the same state, which causes subtle bugs in operational data. NXS keeps them separate.

### Cross-language without friction

All ten readers follow the same lookup strategy:
1. Seek to `EOF - 8` → find the tail-index
2. Binary search the index → get the record's byte offset
3. Read the object header → offset table gives field positions
4. Read the field directly

Because the format is self-describing (schema embedded, sigils declare types), no code generation and no external schema registry are needed. Drop the `.nxb` file anywhere, and any reader can open it.

### When to reach for NXS

| Good fit | Poor fit |
|---|---|
| Large structured datasets read selectively | JSON over HTTP APIs |
| Record-centric random access | Transactional or write-heavy systems |
| Data shared across multiple languages | Columnar analytics (use Parquet/Arrow) |
| Browser tools, CLI tools, internal tooling | Primary database storage |
| Sparse records where fields vary per row | Simple configs with a few keys |
