# ESVG

ESVG is an SVG optimization utility designed to reduce file sizes without compromising perceivable visual quality of rendered vector graphics. It leverages Competitive Transformer Optimization <sup>[1]</sup> to explore multiple optimization strategies and select the most efficient one.

## Competitive Transformer Optimization <sup>[1]</sup>

At the heart of ESVG's optimization process is the Arbiter, which acts as the central orchestrator. The Arbiter passes the input SVG through a series of transformers, each a specialized plugin that applies various transformations aiming to make the file more efficient. These transformers can make a wide range of changes, such as simplifying paths, removing unnecessary elements, applying transformations.

The interesting part of ESVG is its use of competitive optimization. It evaluates multiple transformers simultaneously. After each transformer processes the SVG, the Arbiter compares the results and keeps the transformation that achieves the most efficient output. Efficiency, in this case, is defined as the reduction in file size while ensuring that the rendered vector graphics appear identical or imperceptibly different.

The process works as follows:

- **Input**: The Arbiter receives an SVG file as input.
- **Transformation**: The input SVG is passed through multiple transformers, which apply various optimizations.
- **Comparison**: The Arbiter compares the output from each transformer, assessing which result has the smallest file size while maintaining visual integrity.
- **Selection**: The Arbiter selects the most efficient output and proceeds with it.

This approach ensures that only the most effective transformations are applied, producing the smallest possible file size with no perceivable visible quality loss.

## State

**This project is in very early development. Most of the features are not implemented and it cannot be used effectively for any optimization.**


## Testing

- `cargo test` runs the unit tests and `tests/corpus.rs`, which optimizes every SVG in `tests/cases` and `tests/corpus` and checks that the output parses and is not larger than the input.
- `tests/visual/run.py` checks that the output still looks the same. It renders each original and optimized file with `resvg` and `rsvg-convert`, compares the pixels, and reports the size change. It needs both renderers on `PATH` and the standard library only.

```sh
cargo build --release
tests/visual/run.py
```

`tests/cases` holds hand-written edge cases (CC0). `tests/corpus` holds public-domain SVGs from Wikimedia Commons, listed with their licenses in `tests/corpus/SOURCES.md`.
