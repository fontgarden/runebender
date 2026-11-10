# External workflow worker contract

This is the offline worker protocol for Runebender's graph runner and native Local AI adapter.
For model installation and feature limits, see the [local AI reference](LOCAL-AI.md) and [local model packages](LOCAL-MODELS.md).

## Discovery, Rows inputs, and caching

`runebender graph types --tool /path/to/font-ml --json` exposes the discovered task ports and execution policy.
External tasks can advertise this optional block in each task returned by `tasks --json`:

```json
{"execution":{"schema_version":1,"cache":"input_fingerprint","rows_input":"json_argument"}}
```

Without this block, external tasks remain executable but run every time and reject Rows inputs.
An explicit block requires version 1; unknown versions, fields, or policy values exclude that task from discovery.
The authoring graph file remains version 1.

`json_argument` sends each Rows input as one compact JSON array argument after its port flag, replacing underscores with hyphens in the flag name.
For example, `report_rows` becomes `--report-rows '[{"glyph":"A","score":0.5}]'`; the runner builds argument vectors directly without a shell.
Empty arrays, nested JSON, and Unicode are preserved.
Each input permits at most 4096 rows, and all Rows arguments together permit at most 65536 UTF-8 bytes.
Unsupported or oversized inputs fail before process launch, and a declared Rows output must be a JSON array in the worker's final report.

`input_fingerprint` is an explicit promise by the trusted worker that declared inputs describe its dependencies.
Cache reuse verifies the node definition, executable, input values and files, and retained output content; older cache files are invalidated.
Model and adapter directories are hashed from their actual bytes, including additional artifacts; manifest digests alone are not trusted.
This conservative check can add disk I/O for large local models.
Workers depending on time, random state, environment variables, network responses, or unlisted files should use `cache: "never"` unless those dependencies are supplied as inputs.
External effects are reported as `trusted_process`; this metadata grants no permissions and provides no process sandbox.
Font-writing builtins and live document nodes are never reused from the disk cache.
External declarations of `write_font` or `live_document` likewise force `cache: "never"` while retaining the `trusted_process` classification.

## Native Local AI candidate lifecycle

These are offline worker contracts, not a claim of cloud-provider or model compatibility.
The native Local AI action exports current in-memory data to an owned temporary directory and retains validated output separately until explicit Install.
Install commits the complete candidate as one undo group; Discard leaves the root document unchanged.
This initial adapter accepts ordinary contour point moves, existing anchor moves, and advance changes, with at most 64 captured glyphs and 256 changed values per candidate.
Changed topology, components, hyperbeziers, unsupported metadata, stale results, or out-of-scope glyphs are rejected.
At most 16 candidates are retained, keyed by source and task, for this document session.
An existing saved proposal for the same task must be reviewed or discarded before another model run.
Live DAG model nodes and migration of the older disk-oriented Graph action remain unfinished; the latter still uses its save-first path.
