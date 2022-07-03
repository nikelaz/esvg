# eSVG

eSVG is an optimization utility for Scalable Vector Graphics. It does multiple optimization operations, including:

- Converts all SVG shapes to paths
- Removes translate, scale and matrix transforms from the paths
- Prefixes IDs and classes with hash to avoid naming conflicts when SVGs are used inline
- Performs multiple optimizations with SVGO

This software IS opinionated, because it attempts to provide an easy, no-config-necessary way to optimize SVGs for the web.
Nevertheless, the plan for this project is to expose more options and configuration in future releases.

## Installation

```sh
npm install -g esvg
```

## Usage

```sh
esvg <glob> [options]
```

Arguments:
- `glob` - the input svg file(s) to optimize

Options:
- `-V, --version` - output the version number
- `-p, --pretty` - the output SVG will not me minified
- `-h, --help` - display help for command
