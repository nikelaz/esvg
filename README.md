# eSVG

eSVG is a utility which optimizes SVGs. It combines multiple open-source packages to achieve the following:
- Clean post-processed XML markup
- Converts all SVG shapes to paths
- Removes transforms
- Prefixes IDs and classes with hash to avoid naming conflicts

This utility IS opinionated, because it attempts to provide an easy, no-config-necessary way to optimize SVGs for the web.

## Installation

```sh
npm install -g esvg
```

## Usage

```sh
svgo <glob> [options]
```

Arguments:
- `glob` - The input svg file(s) to optimize

Options:
- `-V, --version` - output the version number
- `-p, --pretty` - Outputs the SVG in a readable format
- `-h, --help` - display help for command
