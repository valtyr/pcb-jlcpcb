# pcb-jlcpcb

A [pcb](https://github.com/nickmass/pcb) subcommand for working with JLCPCB's parts library. Search for components, generate `.zen` files with correct footprints and pinouts, and export BOMs for JLCPCB assembly.

## Installation

```bash
# From crates.io
cargo install pcb-jlcpcb

# Or with cargo-binstall (pre-built binaries)
cargo binstall pcb-jlcpcb
```

## What it does

- **Search** JLCPCB's parts library directly from the terminal
- **Generate** `.zen` component files with footprints pulled from EasyEDA
- **Export** BOMs in JLCPCB's assembly format

The generated components include proper pad mappings extracted from EasyEDA's symbol data, so you don't have to manually figure out pin assignments.

## Usage

```
pcb jlcpcb <COMMAND>

Commands:
  search    Search for parts in the JLCPCB parts library
  generate  Generate .zen component files from JLCPCB parts
  bom       BOM operations for JLCPCB assembly
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Search

Find parts in JLCPCB's catalog:

```
pcb jlcpcb search [OPTIONS] <QUERY>

Arguments:
  <QUERY>  Search query (value, package, category, MPN, etc.)

Options:
  -f, --format <FORMAT>  Output format (human, json) [default: human]
  -b, --basic            Only show JLCPCB basic parts (lower assembly fee)
  -p, --preferred        Include preferred/promotional parts (requires --basic)
  -l, --limit <LIMIT>    Maximum number of results per page [default: 50]
      --page <PAGE>      Page number (1-indexed) [default: 1]
```

Example:

```bash
pcb jlcpcb search "STM32F103" --basic
```

### Generate

Create `.zen` component files from LCSC part numbers:

```
pcb jlcpcb generate [OPTIONS] <LCSC>...

Arguments:
  <LCSC>...  LCSC part number(s) (e.g., C307331)

Options:
  -o, --output <OUTPUT>  Output directory (default: components/JLCPCB/<mpn>/)
  -n, --name <NAME>      Component name override (only for single part)
      --refresh          Ignore cache, re-fetch pins from EasyEDA
```

Example:

```bash
pcb jlcpcb generate C307331 C14858
```

This fetches the component data from JLCPCB/EasyEDA and generates ready-to-use `.zen` files with footprints and pin mappings.

### BOM

Export and check BOMs for JLCPCB assembly:

```
pcb jlcpcb bom <COMMAND>

Commands:
  check   Check BOM availability against JLCPCB inventory
  export  Export BOM in JLCPCB assembly format
```

## License

MIT
