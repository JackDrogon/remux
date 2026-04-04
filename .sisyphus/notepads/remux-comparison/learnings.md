# Project Comparison Learnings

- remux maintains strict CLI flag parity with retmux: -h, -v, -l, -d, -b, -r, -ri, -L.
- The directory structure in src/ maps almost 1:1 to tmuxbk/ in the reference implementation.
- Backup data isolation logic for -L (sockets) is a critical compatibility feature.
