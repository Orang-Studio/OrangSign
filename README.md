# OrangSign (Linux)

native Rust + [GPUI](https://gpui.rs/) rewrite

## Where the certificate lives

1. If cert.pfx exists next to the executable, it is used
2. Otherwise ~/.local/share/OrangStudio/OrangSign/cert.pfx is used.

## Verifying a detached signature

once your certificate is trusted

```sh
openssl cms -verify -binary -inform DER -in <file>.p7s -content <file> -purpose any -out /dev/null
```

## Command line

```sh
orangsign                      # GUI
orangsign --sign <file|folder> # sign
```

## Why tf I made this

for developer teams that need to sign their binaries, packages, or other build artifacts so systems can verify their authenticity. Code signing confirms that what you’re sending the genuine binary for your team and hasn’t been tampered with. Studios would refactor this code and make their own version of it (like changing the CN or something else).