# Forge Craft documentation

- [Getting started](https://github.com/s-b-repo/forgecad/blob/main/README.md#build-from-source)
- [Feature inventory](Feature-status): every tracked repository workflow, implementation status, evidence and limits.
- [Progress](Progress): recorded verification, feature counts and milestone state.
- [Roadmap](Roadmap): completed work, next implementation priorities and acceptance criteria.
- [Product catalog](Product-catalog): all 140 product identities and their current target status.
- [Drafting and plotting](Drafting)
- [Editable modeling](Modeling)
- [Building, terrain, motion, manufacturing and review](Workbenches)
- [Advanced workflows and scripts](Advanced-workflows)
- [File compatibility](Compatibility)
- [Architecture](Architecture)
- [Code audit and remaining findings](Code-audit)
- [Completion requirements](Requirements)
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
