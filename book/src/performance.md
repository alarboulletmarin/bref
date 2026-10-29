# Performance

Measured on the author's machine (Intel Core i9-13900H, Linux, release build), on vaults of 30-line notes linked to each other.

## The app

| Vault | 1,000 notes | 5,000 notes | 20,000 notes |
|---|---|---|---|
| Window on screen | 75 ms | 60 ms | 75 ms |
| Vault indexed, in the background | 20 ms | 90 ms | 275 ms |
| Palette: to open, then per letter | 1.7 ms, 1.5 ms | 3.6 ms, 1.9 ms | 11 ms, 4 ms |
| A note written by another program shows up | in 0.3 to 0.45 s | in 0.3 to 0.45 s | in 0.4 to 0.55 s |
| Memory | 62 MB | 86 MB | 180 MB |

At rest, Bref uses 0.3 % of one processor core.

## One gesture

The time Bref spends between a key and the frame it hands to the graphics card, measured without a display. Drawing comes on top; a screen at 120 Hz leaves 8 ms for both.

| Gesture | Work before the frame is drawn |
|---|---|
| A letter in a short note | 1.1 ms |
| A letter in a note of 1,000 lines | 1.2 ms |
| A letter, an arrow or Enter in a note of 5,000 lines | 1.2 to 1.5 ms |
| A letter in a table of 400 rows | 3.0 ms |
| One notch of the mouse wheel | 1.1 ms |

## Large files

- A table of 1,400 rows with a running total in each row: about 25 ms per keystroke. See [Formulas](tables.md#formulas).
- A table of 300,000 rows in a note opens in about 40 ms, and typing in it takes under 10 ms. See [Very large tables](tables.md#very-large-tables).
- A CSV file of a million rows is parsed in under a tenth of a second, and a million rows cost about 12 MB. See [CSV and TSV files](csv.md).

## Where the figures come from

The first table comes from the running app (window, memory, outside change) and from the scale benchmark (index, palette). The others come from benchmarks run without a display. To measure on your machine:

```bash
cargo test --release --locked -- --ignored --nocapture bench
```

Each benchmark reads its parameters from environment variables named in its doc comment.
