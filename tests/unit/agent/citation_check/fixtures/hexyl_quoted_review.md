## Findings

1. **Panic on invalid braille code point** — src/lib.rs:184 `char::from_u32(0x2800 + to_braille_bits(..)).unwrap()`
   relies on the sum staying inside the braille block.
2. **First byte read without a length check** — src/lib.rs:795 `self.line_buf[0]`
   indexes the buffer before the empty case is handled.
3. Buffer sizing: `src/lib.rs:363`: `line_buf: vec![0x0; 8 * panels as usize]` allocates per panel.
4. Squeeze state: src/lib.rs:713-716 — `self.squeezer = Squeezer::Print` resets after a differing row.
