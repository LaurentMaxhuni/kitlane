import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const app = path.join(repo, 'app');
const run = (command, args) => execFileSync(command, args, { cwd: app, stdio: 'inherit' });
assert(['darwin', 'linux'].includes(process.platform), 'Run on macOS or Linux; Windows uses build.ps1.');
assert(['x64', 'arm64'].includes(process.arch), 'Supported architectures: x64 and arm64.');
const api = new URL(process.env.KITLANE_ACCOUNT_API);
assert(api.protocol === 'https:' && api.href === `${api.origin}/` && !api.username && !api.password,
  'KITLANE_ACCOUNT_API must be an exact HTTPS origin.');
const { packages } = JSON.parse(execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1', '--locked'], { cwd: app }));
const version = packages.find(item => item.name === 'kitlane').version;
assert(/^[0-9]+\.[0-9]+\.[0-9]+$/.test(version), 'Expected a numeric release version.');
if (process.env.GITHUB_REF_TYPE === 'tag') assert.equal(process.env.GITHUB_REF_NAME, `v${version}`, 'Tag must match Cargo version.');
// Rebuild here so the archive cannot accidentally contain an unconfigured executable.
run('cargo', ['build', '--release', '--locked', '--bin', 'kitlane']);
const binary = path.join(app, 'target/release/kitlane');
assert(existsSync(binary), 'Release executable missing.');
const staging = mkdtempSync(path.join(tmpdir(), 'kitlane-package-'));
const output = path.join(repo, 'dist');
mkdirSync(output, { recursive: true });
const name = `Kitlane-${version}-${process.platform === 'darwin' ? 'macOS' : 'Linux'}-${process.arch}`;
const readme = `Kitlane ${version} preview\n\nGoogle sign-in and an internet connection are required.\nAccount service: ${api.origin}\nThis preview uses the development account service and shared development Google OAuth.\nInventory stays local unless you explicitly consent and sync.\nPhysical two-computer and foreign-account browser checks remain pending.\nAutomatic installation and updates are not included.\n\nThis package is unsigned and has not been notarized by Apple.\nDownload only from https://github.com/LaurentMaxhuni/kitlane/releases\nNative packaging and automated tests run in CI; interactive macOS/Linux checks remain pending.\n\n`;
function licenses(destination) {
  mkdirSync(destination, { recursive: true });
  for (const [source, filename] of [
    ['design-system/assets/fonts/Inter-OFL.txt', 'Inter-OFL.txt'],
    ['design-system/assets/fonts/Sora-OFL.txt', 'Sora-OFL.txt'],
    ['design-system/assets/ui-icons/LICENSE', 'Lucide-ISC.txt'],
    ['app/assets/programs/NOTICE.md', 'Program-Logos.md'],
    ['app/installer/licenses/Pelite-MIT.txt', 'Pelite-MIT.txt'],
  ]) copyFileSync(path.join(repo, source), path.join(destination, filename));
}

if (process.platform === 'darwin') {
  const bundle = path.join(staging, 'Kitlane.app/Contents');
  mkdirSync(path.join(bundle, 'MacOS'), { recursive: true });
  mkdirSync(path.join(bundle, 'Resources'), { recursive: true });
  copyFileSync(binary, path.join(bundle, 'MacOS/kitlane'));
  chmodSync(path.join(bundle, 'MacOS/kitlane'), 0o755);
  copyFileSync(path.join(repo, 'design-system/assets/icons/app-icon.icns'), path.join(bundle, 'Resources/kitlane.icns'));
  licenses(path.join(bundle, 'Resources/licenses'));
  writeFileSync(path.join(bundle, 'Info.plist'), `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>kitlane</string>
<key>CFBundleIdentifier</key><string>app.kitlane.desktop</string>
<key>CFBundleName</key><string>Kitlane</string>
<key>CFBundleDisplayName</key><string>Kitlane</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>${version}</string>
<key>CFBundleVersion</key><string>${version}</string>
<key>CFBundleIconFile</key><string>kitlane.icns</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>\n`);
  writeFileSync(path.join(staging, 'README.txt'), readme + 'Requires macOS 12 or newer and OpenGL support.\nOpen the disk image, then drag Kitlane.app into Applications.\nmacOS may block this unsigned preview; see Apple guidance linked on the download page.\n');
  run('plutil', ['-lint', path.join(bundle, 'Info.plist')]);
  run('codesign', ['--force', '--sign', '-', path.join(staging, 'Kitlane.app')]);
  run('codesign', ['--verify', '--deep', '--strict', path.join(staging, 'Kitlane.app')]);
  run('ln', ['-s', '/Applications', path.join(staging, 'Applications')]);
  const disk = path.join(output, `${name}.dmg`);
  run('hdiutil', ['create', '-volname', 'Kitlane', '-srcfolder', staging, '-ov', '-format', 'UDZO', disk]);
  run('hdiutil', ['verify', disk]);
} else {
  const folder = path.join(staging, name);
  mkdirSync(folder);
  copyFileSync(binary, path.join(folder, 'kitlane'));
  chmodSync(path.join(folder, 'kitlane'), 0o755);
  copyFileSync(path.join(repo, 'design-system/assets/icons/app-icon-256.png'), path.join(folder, 'kitlane.png'));
  licenses(path.join(folder, 'licenses'));
  writeFileSync(path.join(folder, 'README.txt'), readme + 'Built on Ubuntu 22.04 (x64) or Ubuntu 24.04 (arm64).\nRequires glibc 2.35+ (x64) or 2.39+ (arm64), a graphical X11/Wayland session, OpenGL, D-Bus, and an unlocked Secret Service keyring (e.g. GNOME Keyring).\nExtract the archive, then run ./kitlane from its folder. No root access required.\nDebian/Ubuntu runtime packages: libdbus-1-3 libxkbcommon0 libwayland-client0 libegl1 libgl1 gnome-keyring xdg-utils\nLinux inventory currently covers Flatpak applications only.\n');
  const archive = path.join(output, `${name}.tar.gz`);
  run('tar', ['-czf', archive, '-C', staging, name]);
  run('tar', ['-tzf', archive]);
}
console.log(`Packaged ${name} with account service ${api.origin}`);
