# Deep dive

Nested documents keep their directory structure in the library and on disk.

## Portable data

Configuration, the SQLite database, fonts and the default library live under `data/` beside the executable in portable mode.

```mermaid
sequenceDiagram
    participant U as User
    participant A as Daftar
    participant S as SQLite
    U->>A: Select text
    A->>A: Relocate anchor
    A->>S: Save highlight
    S-->>A: Persisted
```

[Back to the tour](../english.md)
