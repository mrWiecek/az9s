# Security

## Reporting a vulnerability

Please report security issues privately through
[GitHub's private vulnerability reporting](https://github.com/mrWiecek/c9s/security/advisories/new),
not in a public issue. You'll get an answer within a week; fixes ship as a
patch release with a CHANGELOG entry.

## What c9s protects

c9s is a read-only terminal UI. It runs on your machine with your cloud
CLIs' logins (`az`, `gcloud`, `aws`) and stores no credentials of its own:
tokens live in memory only, cached until shortly before they expire.

Everything a cloud API returns is treated as untrusted, because any
member of a tenant, project or account can name a resource, tag it or
describe a group:

- **Terminal.** No control character or bidirectional override from cloud
  data reaches the terminal: every frame is cleaned before it is drawn, so
  a name can't carry an escape sequence that sets the title, writes the
  clipboard or redraws the screen.
- **Clipboard and spreadsheets.** `y` and `c` copy without control
  characters (a newline in a pasted ID would run what follows). Copied
  tables defuse spreadsheet formulas: a cell starting with `=`, `+`, `-` or
  `@` gets a leading `'`.
- **Browser.** `o` opens only `https://` links to the Azure Portal, the
  Google Cloud console and the AWS console, built with every part
  percent-encoded, and never through a shell (on Windows the URL protocol
  handler opens them, not `cmd.exe`).
- **Cloud CLIs.** `az`, `gcloud` and `aws` are looked up on the PATH only,
  never in the current directory, and started by full path, so on Windows
  their arguments are quoted by Rust's standard library rather than
  parsed by `cmd /C`. Names from config files (tenants, gcloud
  configurations, AWS profiles) with control or shell characters are
  refused.
- **Requests.** Paging links (`nextLink`) are followed only on the API's
  own host, ARM IDs from responses must be paths on it, and an AWS region
  must look like one before it becomes a host name: a token is never sent
  elsewhere. TLS is rustls with current certificate checks.
- **Files.** The cache (inventory, role assignments, directory names) is
  readable by your user only (0700 directory, 0600 files on Unix). Logs are
  off unless `RUST_LOG` is set and go to stderr.

## Supply chain

- Every GitHub Action is pinned to a commit SHA; workflows get read-only
  tokens, and only the release's publish step can write.
- CI runs `cargo audit` against the RustSec database; Dependabot proposes
  dependency and action updates weekly.
- Release archives come with `SHA256SUMS` and signed build provenance:

  ```sh
  gh attestation verify c9s-v1.5.2-x86_64-unknown-linux-gnu.tar.gz --repo mrWiecek/c9s
  ```

## Known, accepted

- `ratatui` 0.26 depends on `lru` 0.12 (RUSTSEC-2026-0002,
  RUSTSEC-2026-0253: unsoundness in `IterMut` and in `pop()` when a panic
  unwinds) and the unmaintained `paste` (RUSTSEC-2024-0436). ratatui uses
  neither path in a way c9s can reach with outside data; they go away with
  the ratatui upgrade.
- The cloud CLIs themselves are trusted: whoever can change `~/.aws/config`
  or the PATH can already run code as you.
