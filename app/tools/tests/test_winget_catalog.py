from pathlib import Path
import runpy
import sqlite3
import tempfile
import unittest
import zipfile

importer = runpy.run_path(str(Path(__file__).parents[1] / "import-winget-catalog.py"))


class ImportTests(unittest.TestCase):
    def setUp(self):
        self.database = sqlite3.connect(":memory:")
        self.addCleanup(self.database.close)
        self.database.executescript("""
            CREATE TABLE metadata(name TEXT, value TEXT);
            INSERT INTO metadata VALUES ('majorVersion','2'), ('minorVersion','0'),
                                        ('lastwritetime','1792073530');
            CREATE TABLE packages(rowid INTEGER PRIMARY KEY, id TEXT, name TEXT);
            INSERT INTO packages VALUES (1,'Example.App','Example'), (2,'Other.App','Other');
            CREATE TABLE productcodes2(productcode TEXT, package INTEGER);
            INSERT INTO productcodes2 VALUES ('Example_is1',1), ('shared',1), ('SHARED',2);
        """)

    def test_exact_codes_are_sorted_and_ambiguous_case_variants_are_omitted(self):
        index = importer["build_index"](self.database, "0" * 64, "2026-10-15")
        self.assertEqual(index["packages"], [("Example.App", "Example")])
        self.assertEqual(index["codes"], [("example_is1", 0)])

    def test_invalid_packages_cannot_hide_product_code_ambiguity(self):
        self.database.execute("UPDATE packages SET name = '' WHERE rowid = 2")
        index = importer["build_index"](self.database, "0" * 64, "2026-10-15")
        self.assertEqual(index["codes"], [("example_is1", 0)])

    def test_unsupported_schema_is_rejected(self):
        self.database.execute("UPDATE metadata SET value = '3' WHERE name = 'majorVersion'")
        with self.assertRaisesRegex(ValueError, "Unsupported"):
            importer["build_index"](self.database, "0" * 64, "2026-10-15")

    def test_invalid_archive_never_replaces_existing_catalog(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "bad.msix"
            output = Path(directory) / "existing.json"
            output.write_text("existing catalog")
            with zipfile.ZipFile(source, "w") as archive:
                archive.writestr("Public/index.db", b"not a database")
            with self.assertRaises(sqlite3.DatabaseError):
                importer["generate"](source, output, "2026-10-15")
            self.assertEqual(output.read_text(), "existing catalog")

    def test_duplicate_package_ids_never_replace_existing_catalog(self):
        self.database.executescript("""
            INSERT INTO packages VALUES (3,'Example.App','Another label');
            INSERT INTO productcodes2 VALUES ('another-code',3);
        """)
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "index.db"
            connection = sqlite3.connect(database)
            try:
                self.database.backup(connection)
            finally:
                connection.close()
            source = Path(directory) / "duplicate.msix"
            with zipfile.ZipFile(source, "w") as archive:
                archive.write(database, "Public/index.db")
            output = Path(directory) / "existing.json"
            output.write_text("existing catalog")
            with self.assertRaisesRegex(ValueError, "Duplicate"):
                importer["generate"](source, output, "2026-10-15")
            self.assertEqual(output.read_text(), "existing catalog")

    def test_dates_must_satisfy_runtime_catalog_validation(self):
        with self.assertRaisesRegex(ValueError, "date"):
            importer["build_index"](self.database, "0" * 64, "2019-10-15")


if __name__ == "__main__":
    unittest.main()
