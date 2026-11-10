#!/usr/bin/env python3
"""Generate repository and GitHub-wiki guides from one reviewed inventory.

No network writes. --check verifies generated pages, managed links and inputs.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
REPO = "https://github.com/s-b-repo/forgecad"
LABELS = {"implemented": "Implemented", "partial": "Partial", "planned": "Planned", "external": "External evidence"}
STATES = {"verified": "Verified within documented bounds", "in_progress": "In progress", "planned": "Planned", "external": "Pending external acceptance data"}


def load_data():
    inventory = json.loads((DOCS / "feature-status.json").read_text())
    roadmap = json.loads((DOCS / "roadmap.json").read_text())
    verification = json.loads((ROOT / "validation/results/verification.json").read_text())
    products = json.loads((ROOT / "crates/forge-core/data/products.json").read_text())
    features = inventory["features"]
    ids = [f["id"] for f in features]
    assert len(ids) == len(set(ids)) and all(re.fullmatch(r"[a-z0-9-]+", i) for i in ids)
    for feature in features:
        assert feature["status"] in LABELS and feature["coverage"] and feature["limits"]
        for source in feature["evidence"]:
            assert (ROOT / source).is_file(), source
    covered = set()
    milestones = set()
    for milestone in roadmap["milestones"]:
        assert milestone["id"] not in milestones and milestone["status"] in STATES
        milestones.add(milestone["id"])
        assert set(milestone["features"]) <= set(ids)
        covered.update(milestone["features"])
    assert covered == set(ids), f"Unplanned features: {set(ids)-covered}"
    assert len(products) == len({p["id"] for p in products}) == 140
    assert verification["workspace_tests_failed"] == 0
    return inventory, roadmap, verification, products


def table_cell(value):
    return str(value).replace("|", "\\|").replace("\n", " ")


def feature_page(inventory):
    out = ["# Feature inventory", "", inventory["scope"], "", "This inventory covers current repository workflows and planned capability groups. The separate product catalog retains all 140 target identities; each still needs a release/platform-specific acceptance inventory.", "", "Status means: **Implemented** = usable within the stated limits; **Partial** = part of the named capability is available; **Planned** = implementation remains; **External evidence** = acceptance data or originating applications are still required.", ""]
    for area in dict.fromkeys(f["area"] for f in inventory["features"]):
        out += [f"## {area}", "", "| ID / feature | Status | Current coverage | Limits / remaining work | Evidence |", "| --- | --- | --- | --- | --- |"]
        for f in inventory["features"]:
            if f["area"] != area:
                continue
            evidence = ", ".join(f"[{Path(p).name}]({REPO}/blob/main/{p})" for p in f["evidence"]) or "—"
            values = [f"`{f['id']}` · {f['name']}", LABELS[f["status"]], f["coverage"], f["limits"], evidence]
            out.append("| " + " | ".join(map(table_cell, values)) + " |")
        out.append("")
    return "\n".join(out)


def roadmap_page(roadmap):
    out = ["# Progress plan", "", roadmap["completion_rule"], "", "The next implementation priorities are persistent sketch constraints, dependent solid features/booleans, and expanded BIM/civil systems. Milestones below describe acceptance gates; catalog entries or launcher routes do not satisfy them.", ""]
    for m in roadmap["milestones"]:
        out += [f"## {m['id']} — {m['title']}", "", f"**Status:** {STATES[m['status']]}", "", m["acceptance"], "", "Tracked features: " + ", ".join(f"`{f}`" for f in m["features"]) + ".", ""]
    out += ["## Updating this plan", "", "Add or update feature evidence and limits in `feature-status.json`. Move a milestone to verified only after its acceptance cases pass and the current source/build hashes are recorded. Keep incomplete capabilities and external verification requirements explicit. Generate this page and the wiki with `python3 scripts/generate_docs.py`.", ""]
    return "\n".join(out)


def progress_page(inventory, roadmap, verification, products):
    counts = Counter(f["status"] for f in inventory["features"])
    product_counts = Counter(p["status"] for p in products)
    out = ["# Project progress", "", "Forge Craft remains an incomplete counterpart to the full supplied product catalog. These counts describe the repository's workflow inventory, not a completion percentage for Autodesk products.", "", "## Recorded verification", "", f"- {verification['workspace_tests_passed']} workspace tests passed; {verification['workspace_tests_failed']} failed.", f"- {len(verification['native_startup_cases'])} populated native workbench launch/render cases.", f"- {len(verification['native_interaction_cases'])} native drafting/edit/save/reopen interaction checks.", f"- {len(verification['advanced_workflow_cases'])} documented workflow groups with generated artifacts.", f"- Native project format: v{verification['native_project_format_version']}, with older-version migration.", f"- Clippy: {verification['clippy_status']}.", "", f"Exact source/artifact hashes and generation time are in [verification.json]({REPO}/blob/main/validation/results/verification.json). Native screenshots prove startup/rendering; domain tests cover their stated geometry, relationships and export cases. They do not establish full product parity, Windows/macOS release quality or machine acceptance.", "", "## Feature inventory counts", "", "| Status | Workflow rows |", "| --- | ---: |"]
    out += [f"| {label} | {counts[status]} |" for status, label in LABELS.items()]
    out += ["", f"Total: {len(inventory['features'])} tracked workflow groups. [Full feature inventory](features.md).", "", "## Product targets", "", f"{len(products)} distinct targets: " + ", ".join(f"{count} {status}" for status, count in sorted(product_counts.items())) + ". No target is marked as a complete product replacement. [All product identities](product-catalog.md).", "", "## Milestones", "", "| Milestone | State |", "| --- | --- |"]
    out += [f"| {m['id']} · {m['title']} | {STATES[m['status']]} |" for m in roadmap["milestones"]]
    out += ["", "[Acceptance criteria and next work](roadmap.md). Proprietary native formats, full sketch/assembly systems, engineering simulation, production media and cloud services remain substantial open work.", ""]
    if "code_audit" in verification:
        audit=verification["code_audit"]
        out += ["## Code audit", "", f"The supplied script scanned {audit['files_scanned']} Rust files. Its raw strict gate is {'clean' if audit['raw_strict_passed'] else 'not clean'} with {audit['strict_hits']} overlapping strict pattern hits, including test assertions and legacy invariants. New production issues fixed this round and remaining audit debt are described in [Code audit](code-audit.md). These counts are not a count of confirmed vulnerabilities.", ""]
    return "\n".join(out)


def catalog_page(products):
    out = ["# Product target catalog", "", "All 140 identities are retained from the supplied catalog. Workbench routing shares infrastructure; it does not establish the distinct features, APIs, native formats or service workflows of any target. Product-release/platform acceptance inventories remain unreviewed.", "", "| ID | Product | Workbench | Kind | Lifecycle | Implementation target status |", "| --- | --- | --- | --- | --- | --- |"]
    for p in sorted(products, key=lambda p: p["name"].casefold()):
        out.append("| " + " | ".join(table_cell(p[k]).replace("_", " ") for k in ["id", "name", "area", "kind", "lifecycle", "status"]) + " |")
    out += ["", "[Repository feature status](features.md) · [Native-file requirements](compatibility.md) · [Full completion requirements](requirements.md)", ""]
    return "\n".join(out)


PAGES = {
    "README.md": "Documentation", "features.md": "Feature-status", "progress.md": "Progress", "roadmap.md": "Roadmap",
    "product-catalog.md": "Product-catalog", "drafting.md": "Drafting", "modeling.md": "Modeling", "workbenches.md": "Workbenches",
    "advanced-workflows.md": "Advanced-workflows", "compatibility.md": "Compatibility", "architecture.md": "Architecture", "requirements.md": "Requirements", "code-audit.md": "Code-audit",
}


def wiki_links(text):
    def replace(match):
        label, target = match.groups()
        if target.startswith(("https://", "http://", "#")):
            return match.group(0)
        base, _, anchor = target.partition("#")
        if base.startswith("docs/"):
            base = base[5:]
        if base in PAGES:
            destination = PAGES[base] + ("#" + anchor if anchor else "")
        elif base.startswith("../"):
            destination = REPO + "/blob/main/" + base[3:] + ("#" + anchor if anchor else "")
        else:
            destination = REPO + "/blob/main/docs/" + target
        return f"[{label}]({destination})"
    return re.sub(r"\[([^\]]+)\]\(([^)]+)\)", replace, text)


def section(text, name):
    match = re.search(r"^## " + re.escape(name) + r"\n(.*?)(?=^## |\Z)", text, flags=re.M | re.S)
    assert match, name
    return match.group(1).strip()


def generate():
    inventory, roadmap, verification, products = load_data()
    outputs = {
        DOCS / "features.md": feature_page(inventory), DOCS / "roadmap.md": roadmap_page(roadmap),
        DOCS / "progress.md": progress_page(inventory, roadmap, verification, products), DOCS / "product-catalog.md": catalog_page(products),
    }
    for name, wiki_name in PAGES.items():
        text = outputs.get(DOCS / name, (DOCS / name).read_text() if (DOCS / name).exists() else "")
        outputs[DOCS / "wiki" / f"{wiki_name}.md"] = wiki_links(text).rstrip() + "\n"
    readme = (ROOT / "README.md").read_text()
    outputs[DOCS / "wiki/Getting-started.md"] = "# Getting started\n\n" + wiki_links(section(readme, "Build from source")) + "\n\n" + wiki_links(section(readme, "Run the built Linux applications")) + "\n\n[Modeling](Modeling) · [Workbenches](Workbenches) · [Compatibility](Compatibility)\n"
    workbenches = (DOCS / "workbenches.md").read_text()
    for heading, name in [("Building", "Building"), ("Terrain", "Terrain"), ("Motion", "Motion"), ("Manufacturing", "Manufacturing"), ("Local project review", "Project-review")]:
        outputs[DOCS / "wiki" / f"{name}.md"] = f"# {heading}\n\n" + wiki_links(section(workbenches, heading)) + "\n\n[Advanced workflows](Advanced-workflows) · [Feature status](Feature-status)\n"
    outputs[DOCS / "wiki/Home.md"] = "# Forge Craft\n\nA native Rust CAD and engineering suite with Draft, Model, Build, Terrain, Make, Motion and Connect workbenches. The complete 140-product acceptance objective remains open.\n\n" + f"Recorded verification: **{verification['workspace_tests_passed']} tests passed**, seven native workbench views and native drafting interactions. [Detailed progress](Progress).\n\n" + "\n".join(f"- [{label}]({target})" for label, target in [("Getting started", "Getting-started"), ("Complete feature inventory", "Feature-status"), ("Progress and verification", "Progress"), ("Roadmap and acceptance plan", "Roadmap"), ("All 140 product targets", "Product-catalog"), ("Drafting and plotting", "Drafting"), ("Editable modeling", "Modeling"), ("Building", "Building"), ("Terrain and civil analysis", "Terrain"), ("Manufacturing", "Manufacturing"), ("Motion and animation", "Motion"), ("Local project review", "Project-review"), ("Automation and advanced workflows", "Advanced-workflows"), ("File compatibility", "Compatibility"), ("Architecture", "Architecture"), ("Full completion requirements", "Requirements")]) + f"\n\n[Source repository]({REPO}). Wiki pages are generated from versioned documentation; implementation and compatibility limits stay explicit.\n"
    outputs[DOCS / "wiki/_Sidebar.md"] = "**Forge Craft**\n\n" + "\n".join(f"- [{name.replace('-', ' ')}]({name})" for name in ["Home", "Getting-started", "Feature-status", "Progress", "Roadmap", "Product-catalog", "Drafting", "Modeling", "Building", "Terrain", "Manufacturing", "Motion", "Project-review", "Advanced-workflows", "Compatibility", "Code-audit", "Architecture", "Requirements"]) + "\n"
    outputs[DOCS / "wiki/_Footer.md"] = f"[Forge Craft source]({REPO}) · [Progress](Progress) · [Compatibility](Compatibility) · [Code audit](Code-audit). Prototype workflows have explicit bounds; catalog identities do not establish product parity.\n"
    # Validate wiki-local destinations before publishing any page.
    wiki_names = {p.stem for p in outputs if p.parent == DOCS / "wiki"}
    for path, text in outputs.items():
        if path.parent != DOCS / "wiki":
            continue
        for target in re.findall(r"\[[^\]]+\]\(([^)]+)\)", text):
            if not target.startswith(("http://", "https://", "#")):
                assert target.partition("#")[0] in wiki_names, (path.name, target)
    return {p: text.rstrip() + "\n" for p, text in outputs.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    options = parser.parse_args()
    outputs = generate()
    stale = []
    for path, content in outputs.items():
        if options.check:
            if not path.exists() or path.read_text() != content:
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
    if stale:
        raise SystemExit("Generated documentation is stale: " + ", ".join(stale))
    print(f"{'Checked' if options.check else 'Generated'} {len(outputs)} documentation pages; feature IDs, evidence paths, catalog identities, milestones and wiki links validated")


if __name__ == "__main__":
    main()
