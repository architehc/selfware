## How hexyl renders a line

The `Printer` struct (`src/lib.rs:120`) owns the output state. Squeezing of
repeated lines is decided per row (`src/lib.rs:715`; buffer allocated at
`src/lib.rs:363`), so identical rows collapse into a single `*` marker.

`print_all` (`src/lib.rs:410`) drives the loop: it fills the line buffer,
compares it with the previous one (see src/lib.rs:795) and hands each panel to
the byte formatter (src/lib.rs:180-190). Braille output maps two bytes into
one glyph (src/lib.rs:184).
