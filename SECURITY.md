# Security policy

## Reporting a vulnerability

Please report security problems privately, through GitHub: open the repository's [Security tab](https://github.com/gianniacquisto/tandem/security) and choose **Report a vulnerability**, or go straight to the [private report form](https://github.com/gianniacquisto/tandem/security/advisories/new). Only the maintainers see the report.

Do not open a public issue, pull request or discussion for a vulnerability.

A useful report says:

- the release (`tandem --version`) and the platform;
- how the server was started: loopback, `--allow-unauthenticated`, or token mode, and whether the home is in team mode;
- the steps to reproduce, and what an attacker gains.

## Supported versions

Only the latest release receives security fixes. A fix ships as a new release; upgrade with the installer's `--reinstall` mode ([installing and upgrading](docs/install-paths.md)).

## Scope

In scope is anything that lets someone read or change a board, its notes or its people without the credential the [HTTP API reference](docs/http-api.md#authentication) says they need. For example:

- bypassing the owner token, a sign-in key, a personal token or the operator key;
- a personal token reaching beyond `/mcp`, or a member gaining an admin's permissions;
- a cross-site request that writes to the board or downloads the export;
- reading or writing files outside the home through a note path or an import archive;
- a release archive or `install.sh` that does not match its published checksum or provenance.

Out of scope, because they are documented trust decisions:

- anything reachable on a loopback server from the same machine: loopback trusts the machine;
- anything reachable on a server started with `--allow-unauthenticated`;
- what someone can do with access to the home's files on disk, which are the boundary for the store, the backup and the notes;
- the export archive not being encrypted.

The design behind these boundaries is in [ADR 0047](docs/adr/0047-serve-is-the-single-door.md), [ADR 0049](docs/adr/0049-a-browser-signs-in-to-a-token-guarded-serve.md) and [ADR 0052](docs/adr/0052-members-and-personal-tokens.md).
