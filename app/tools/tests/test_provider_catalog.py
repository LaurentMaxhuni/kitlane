import importlib.util
import gzip
import json
from pathlib import Path
import tempfile
import unittest

path = Path(__file__).parents[1] / 'import-provider-catalog.py'
spec = importlib.util.spec_from_file_location('providers', path)
importer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(importer)


def cask(token='example', bundle='com.example.App', homepage='https://example.org/app'):
    return {'token': token, 'name': ['Example'], 'homepage': homepage,
            'disabled': False, 'deprecated': False,
            'artifacts': [{'uninstall': [{'quit': bundle}]}]}


class ProviderTests(unittest.TestCase):
    def test_bundle_identifiers_are_exact_and_ambiguous_ids_are_removed(self):
        packages = importer.homebrew([cask(), cask('other')])
        self.assertTrue(all(not p['native_ids'] for p in packages))
        package = importer.homebrew([cask()])[0]
        self.assertEqual(package['native_ids'], ['com.example.App'])
        self.assertEqual(importer.homebrew([cask(bundle='com.example.*')])[0]['native_ids'], [])

    def test_disabled_casks_and_non_application_flatpaks_are_excluded(self):
        disabled = cask()
        disabled['disabled'] = True
        self.assertEqual(importer.homebrew([disabled]), [])
        xml = b'<components><component type="runtime"><id>org.example.Runtime</id><name>Example</name></component></components>'
        self.assertEqual(importer.flathub(xml), [])

    def test_same_name_is_insufficient_for_cross_platform_identity(self):
        mac = importer.homebrew([cask()])
        linux = importer.flathub(b'<components><component type="desktop-application"><id>org.example.App</id><name>Example</name><url type="homepage">https://other.org/app</url><bundle type="flatpak">app/org.example.App/x86_64/stable</bundle></component></components>')
        importer.link_packages(mac + linux, {})
        self.assertNotEqual(mac[0]['canonical_id'], linux[0]['canonical_id'])

    def test_stable_flatpak_never_links_to_homebrew_prerelease_or_versioned_cask(self):
        for token in ['example@insiders', 'example@2', 'example-nightly', 'example-beta']:
            mac = importer.homebrew([cask(token=token)])
            linux = importer.flathub(b'<components><component type="desktop-application"><id>org.example.App</id><name>Example</name><url type="homepage">https://example.org/app</url><bundle type="flatpak">app/org.example.App/x86_64/stable</bundle></component></components>')
            importer.link_packages(mac + linux, {})
            self.assertNotEqual(mac[0]['canonical_id'], linux[0]['canonical_id'], token)

    def test_unique_exact_homepage_and_name_cross_reference_both_sources(self):
        mac = importer.homebrew([cask()])
        linux = importer.flathub(b'<components><component type="desktop-application"><id>org.example.App</id><name>Example</name><url type="homepage">https://example.org/app/</url><bundle type="flatpak">app/org.example.App/x86_64/stable</bundle></component></components>')
        importer.link_packages(mac + linux, {})
        self.assertEqual(mac[0]['canonical_id'], linux[0]['canonical_id'])

    def test_invalid_source_never_replaces_existing_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'homebrew').write_text('[]', encoding='utf-8')
            (root / 'flathub').write_bytes(b'not gzip')
            (root / 'links').write_text('{}', encoding='utf-8')
            output = root / 'index.json'
            output.write_text('existing', encoding='utf-8')
            with self.assertRaises(ValueError):
                importer.generate(root / 'homebrew', root / 'flathub', root / 'links', output, '2026-10-16')
            self.assertEqual(output.read_text(encoding='utf-8'), 'existing')

    def test_duplicate_ids_unknown_links_and_credential_urls_are_rejected(self):
        with self.assertRaises(ValueError):
            importer.homebrew([cask(), cask()])
        packages = importer.homebrew([cask()])
        with self.assertRaises(ValueError):
            importer.link_packages(packages, {'homebrew:missing': 'example'})
        with self.assertRaises(ValueError):
            importer.homebrew([cask(homepage='https://user:secret@example.org')])

    def test_appstream_uses_default_name_and_stable_bundle_id(self):
        xml = b'<components><component type="desktop-application"><id>org.example.App.desktop</id><name xml:lang="fr">Exemple</name><name>Example</name><bundle type="flatpak">app/org.example.App/x86_64/stable</bundle></component></components>'
        package = importer.flathub(xml)[0]
        self.assertEqual(package['id'], 'org.example.App')
        self.assertEqual(package['name'], 'Example')
        self.assertEqual(package['native_ids'], ['org.example.App'])

    def test_doctype_and_runtime_or_mismatched_refs_are_not_imported(self):
        with self.assertRaises(ValueError):
            importer.flathub(b'<!DOCTYPE components><components/>')
        xml = b'<components><component type="desktop-application"><id>org.example.App</id><name>Example</name><bundle type="flatpak">runtime/org.example.App/x86_64/stable</bundle></component></components>'
        self.assertEqual(importer.flathub(xml), [])

    def test_refresh_is_deterministic_and_invalid_date_preserves_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            brew, flatpak, links, output = [root / name for name in ('brew', 'flatpak', 'links', 'output')]
            brew.write_text(json.dumps([cask()]), encoding='utf-8')
            flatpak.write_bytes(gzip.compress(b'<components/>'))
            links.write_text('{}', encoding='utf-8')
            importer.generate(brew, flatpak, links, output, '2026-10-16')
            first = output.read_bytes()
            importer.generate(brew, flatpak, links, output, '2026-10-16')
            self.assertEqual(output.read_bytes(), first)
            with self.assertRaises(ValueError):
                importer.generate(brew, flatpak, links, output, '2026-02-30')
            self.assertEqual(output.read_bytes(), first)


if __name__ == '__main__':
    unittest.main()
