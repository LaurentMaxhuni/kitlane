"""Build Kitlane's offline product-code index from Microsoft's source2.msix.

Python 3.11+, stdlib only. No installers, network requests, or registry access.
"""
import argparse
from collections import defaultdict
from datetime import date, datetime, timezone
import hashlib
import json
import re
from pathlib import Path
import sqlite3
import tempfile
import zipfile

SOURCE = "https://cdn.winget.microsoft.com/cache/source2.msix"
MAX_SOURCE_BYTES = 64 * 1024 * 1024
MAX_OUTPUT_BYTES = 32 * 1024 * 1024


def metadata_key(value):
    if not valid_text(value, 512) or not value.isascii():
        return None
    key = re.sub(r'[^a-z0-9+#@]', '', value.lower())
    return key or None


def valid_text(value, limit):
    return (isinstance(value, str) and 0 < len(value.encode("utf-8")) <= limit
            and value == value.strip() and all(c.isprintable() for c in value))


def build_index(connection, source_hash, generated_on):
    metadata = dict(connection.execute("SELECT name, value FROM metadata"))
    if (metadata.get("majorVersion"), metadata.get("minorVersion")) != ("2", "0"):
        raise ValueError("Unsupported WinGet index schema")
    updated = datetime.fromtimestamp(int(metadata["lastwritetime"]), timezone.utc).date()
    generated = date.fromisoformat(generated_on)
    if updated.year < 2020 or generated.year < 2020:
        raise ValueError("Catalog date must be 2020 or later")
    packages = {}
    identifiers = set()
    for count, (rowid, identifier, name) in enumerate(connection.execute("SELECT rowid, id, name FROM packages")):
        if count >= 100_000:
            raise ValueError("Package count limit exceeded")
        if (valid_text(identifier, 256) and identifier.isascii()
                and all(c.isalnum() or c in "._-+" for c in identifier)
                and valid_text(name, 512)):
            if identifier in identifiers:
                raise ValueError("Duplicate WinGet package identifier")
            identifiers.add(identifier)
            packages[rowid] = (identifier, name)
    owners = defaultdict(set)
    for count, (code, rowid) in enumerate(connection.execute(
            "SELECT productcode, package FROM productcodes2")):
        if count >= 500_000:
            raise ValueError("Product-code count limit exceeded")
        if valid_text(code, 256) and code.isascii():
            owners[code.lower()].add(rowid)
    # Windows registry key comparison is case insensitive. Ambiguous codes never match.
    unique = {code: next(iter(ids)) for code, ids in owners.items()
              if len(ids) == 1 and next(iter(ids)) in packages}
    names, publishers = defaultdict(set), defaultdict(set)
    for table, column, target in [('norm_names2', 'norm_name', names),
                                  ('norm_publishers2', 'norm_publisher', publishers)]:
        for count, (value, rowid) in enumerate(connection.execute(f'SELECT {column}, package FROM {table}')):
            if count >= 500_000:
                raise ValueError('Metadata row limit exceeded')
            key = metadata_key(value)
            if key:
                target[rowid].add(key)
                if len(target[rowid]) > 4096:
                    raise ValueError('Metadata alias limit exceeded')
    pair_owners = defaultdict(set)
    count = 0
    for rowid in names.keys() & publishers.keys():
        for name in names[rowid]:
            for publisher in publishers[rowid]:
                count += 1
                if count > 500_000:
                    raise ValueError('Metadata pair limit exceeded')
                pair_owners[(name, publisher)].add(rowid)
    pairs = {pair: next(iter(ids)) for pair, ids in pair_owners.items()
             if len(ids) == 1 and next(iter(ids)) in packages}
    used = sorted(packages, key=lambda key: packages[key][0])
    positions = {key: index for index, key in enumerate(used)}
    if not unique:
        raise ValueError("No unambiguous product codes")
    return {
        "schema_version": 2,
        "source_url": SOURCE,
        "source_sha256": source_hash,
        "source_updated_on": str(updated),
        "generated_on": str(generated),
        "packages": [packages[key] for key in used],
        "codes": [(code, positions[key]) for code, key in sorted(unique.items())],
        "pairs": [(name, publisher, positions[key]) for (name, publisher), key in sorted(pairs.items())],
    }


def generate(source, output, generated_on):
    if source.stat().st_size > MAX_SOURCE_BYTES:
        raise ValueError("Source archive byte limit exceeded")
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix="kitlane-winget-") as directory:
        database = Path(directory) / "index.db"
        with zipfile.ZipFile(source) as archive:
            entry = archive.getinfo("Public/index.db")
            if entry.file_size > MAX_SOURCE_BYTES:
                raise ValueError("Source database byte limit exceeded")
            database.write_bytes(archive.read(entry))
        connection = sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)
        try:
            connection.execute("PRAGMA trusted_schema=OFF")
            index = build_index(connection, source_hash, generated_on)
        finally:
            connection.close()
    # Stable ordering makes refreshes reviewable. Replace only after complete validation.
    arrays = {key: index.pop(key) for key in ("packages", "codes", "pairs")}
    parts = [json.dumps(index, indent=2, ensure_ascii=False)[:-2]]
    for key, rows in arrays.items():
        parts.append(f',\n  "{key}": [\n')
        parts.append(",\n".join("    " + json.dumps(row, ensure_ascii=False) for row in rows))
        parts.append("\n  ]")
    payload = ("".join(parts) + "\n}\n").encode("utf-8")
    if len(payload) > MAX_OUTPUT_BYTES:
        raise ValueError("Output byte limit exceeded")
    json.loads(payload)
    with tempfile.NamedTemporaryFile(dir=output.parent, delete=False, suffix=".tmp") as handle:
        temporary = Path(handle.name)
        try:
            handle.write(payload)
            handle.close()
            temporary.replace(output)
        finally:
            temporary.unlink(missing_ok=True)
    print(f"{len(arrays['packages'])} packages; {len(arrays['codes'])} exact codes; "
          f"{len(arrays['pairs'])} metadata pairs; {len(payload)} bytes; source SHA-256 {source_hash}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--generated-on", required=True, help="YYYY-MM-DD")
    args = parser.parse_args()
    generate(args.source, args.output, args.generated_on)
