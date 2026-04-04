# Architectural Decisions

- Configuration: Moved from custom .conf to config.toml for Rust ecosystem compatibility.
- Error Handling: Implemented a centralized Error type in src/error.rs to replace Python exceptions.
- Domain Mapping: src/model.rs holds the canonical Tmux, Session, Window, and Pane structs.
