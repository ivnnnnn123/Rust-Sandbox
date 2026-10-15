# Rust Sandbox - Proiect Sisteme de Operare

Acest proiect implementează un mini sandbox în Rust care rulează programe externe într-un mediu controlat.

## Funcționalități

- rulează procese copil
- limitează CPU la 2 secunde
- limitează memoria la 64 MB
- blochează syscall-uri de rețea folosind seccomp
- afișează log-uri despre execuție

## Exemple de rulare

```bash
cargo run -- /bin/echo Salut
cargo run -- /bin/ls -la
cargo run -- /usr/bin/python3 memory_test.py
cargo run -- /usr/bin/python3 -c "import socket; socket.socket()"
