# Local project review

Connect retains local issues with titles, multiline descriptions, human-readable element references, priorities and Open / In progress / Resolved states. Search and status filters narrow the board. Staged edits, creation and deletion validate before one undo step; native saves preserve issue text and unrelated model data. Element references are descriptive text rather than checked model links.

```sh
./target/desktop/debug/forge workbench create connect target/review.forge.json
./target/desktop/debug/forge-craft --product autodesk-docs --open target/review.forge.json
./target/desktop/debug/forge workbench export target/review.forge.json target/issues.csv --area connect
```

CSV escapes multiline/quoted text and prefixes formula-like user cells for spreadsheet use. The native project retains exact text. This is a local board with no messaging, user accounts, permissions, version-control server or cloud synchronization. Those services and Autodesk service compatibility remain unfinished.

[Advanced workflows](Advanced-workflows) · [Feature status](Feature-status)
