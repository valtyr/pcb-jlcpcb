# pcb-jlcpcb

A [pcb](https://github.com/diodeinc/pcb) subcommand for working with JLCPCB's parts library. Search for components, generate `.zen` files with correct footprints and pinouts, and export BOMs for JLCPCB assembly.

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

#### `bom check`

Check BOM availability against JLCPCB inventory:

```
pcb jlcpcb bom check [OPTIONS] <BOM>

Arguments:
  <BOM>  Path to BOM file (.json or .zen)

Options:
  -q, --quantity <QUANTITY>  Quantity of boards to build [default: 100]
      --include-dnp          Include DNP (Do Not Place) components
  -f, --format <FORMAT>      Output format (human, json) [default: human]
      --refresh              Bypass the 24-hour part cache
```

Example:

```bash
pcb jlcpcb bom check my-board.zen --quantity 50
```

#### `bom export`

Export BOM in JLCPCB assembly CSV format:

```
pcb jlcpcb bom export [OPTIONS] <BOM>

Arguments:
  <BOM>  Path to BOM file (.json or .zen)

Options:
  -o, --output <OUTPUT>  Output CSV file path [default: jlcpcb_bom.csv]
      --include-dnp      Include DNP (Do Not Place) components
  -f, --format <FORMAT>  Output format (human, json) [default: human]
      --refresh          Bypass the 24-hour part cache
```

Example:

```bash
pcb jlcpcb bom export my-board.zen -o assembly_bom.csv
```

Part lookup results are cached locally for 24 hours (`~/.pcb/jlcpcb/parts/`). Use `--refresh` to bypass the cache and fetch fresh data from the API.

### Utilities

Manage local caches and other housekeeping tasks:

```
pcb jlcpcb util <COMMAND>

Commands:
  clean-cache  Clear cached API data
```

#### `util clean-cache`

Clear locally cached part and pin data:

```
pcb jlcpcb util clean-cache [OPTIONS]

Options:
      --parts  Only clear the part lookup cache
      --pins   Only clear the pin extraction cache
```

When neither flag is given, both caches are cleared.

Example:

```bash
pcb jlcpcb util clean-cache          # clear all caches
pcb jlcpcb util clean-cache --parts  # clear only part cache
pcb jlcpcb util clean-cache --pins   # clear only pin cache
```

## License

MIT
