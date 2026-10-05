# Contributing

Thanks for helping make binary analysis easier to learn.

For GUI problems, open an issue in this repository with your operating system, GUI version, steps to reproduce, and the expected result. Remove personal paths and sensitive data from logs before sharing them. Please do not upload proprietary binaries or private disassembly output.

For an engine problem, first reproduce it with the pinned RADD command-line engine. Include the engine revision and a small redistributable example when reporting a reproducible engine issue to [PNNL RADD](https://github.com/pnnl/radd). Questions about this GUI belong here.

For a code change, explain the user-facing problem, keep the change focused, and run `cargo test --locked`. UI changes should include a screenshot. Platform fixes should identify the OS and CPU actually tested. See README.md for build prerequisites and real-engine tests.

Contributions use the project's MIT license. Upstream engine and dependency code retains its original licenses.
