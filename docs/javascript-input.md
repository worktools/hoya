# JavaScript input binding

`POST /execute/js` accepts JSON values for `input` and `datasource`.
The engine parses each value with QuickJS's JSON parser and passes it as a
function argument. It does not embed JSON text into executable source.
Omitted values default to `{}`; explicit `null`, scalars, arrays, and objects
are preserved. `datasource` is available as `ctx.datasource`.

Backticks, `${...}`, backslashes, newlines, Unicode, and keys such as
`__proto__` are data. No string escaping is required beyond normal JSON
encoding by the client. This holds for synchronous `main` and supported
Promise-returning functions. This change does not add asynchronous network
or timer support, or change the legacy response envelope.

Run the regressions without starting a server:

```sh
cargo test --locked js_engine::tests
```

The tests round-trip nested and primitive values through both entry styles,
check that template expressions are not evaluated, and verify that a rejected
Promise is still an execution error.
