# Forge Craft documentation

- [Getting started](../README.md#build-from-source)
- [Feature inventory](features.md): every tracked repository workflow, implementation status, evidence and limits.
- [Progress](progress.md): recorded verification, feature counts and milestone state.
- [Roadmap](roadmap.md): completed work, next implementation priorities and acceptance criteria.
- [Product catalog](product-catalog.md): all 140 product identities and their current target status.
- [Drafting and plotting](drafting.md)
- [Editable modeling](modeling.md)
- [Building, terrain, motion, manufacturing and review](workbenches.md)
- [Advanced workflows and scripts](advanced-workflows.md)
- [File compatibility](compatibility.md)
- [Architecture](architecture.md)
- [Code audit and remaining findings](code-audit.md)
- [Completion requirements](requirements.md)
- [Published wiki](https://github.com/s-b-repo/forgecad/wiki)

The wiki is generated from these versioned sources. Update `feature-status.json`, `roadmap.json` and the relevant guide with each feature. Refresh test, build, native, workflow and code-audit reports, then run:

```sh
python3 validation/summarize.py
python3 scripts/generate_docs.py
python3 validation/summarize.py
python3 scripts/generate_docs.py --check
```

The second summary binds the freshly generated documentation to the recorded source hashes. Progress pages use stable verification counts, so this does not change their content. Copy the generated `docs/wiki/*.md` pages to the wiki checkout, review its diff and publish that checkout alongside the matching repository update.

Generated progress counts describe the workflow inventory. They are not a percentage of Autodesk parity. The complete product acceptance objective remains open.
