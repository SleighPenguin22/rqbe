# RQBE

An (attempt at an) implementation of QBE, written in Rust because "how hard could it be"

The current goal is for rQBE to emit assembly, the same way QBE does. Later on, I might try to either integrate or build an assembler for
x86_64 and Aarch64, as those are the machines I have access to.

The IL is also A little more permissive, it does not require an empty `@start` label as the first label.
