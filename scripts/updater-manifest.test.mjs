import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { buildManifest, stagePlan, verifySignature } from './updater-manifest.mjs';

const script = fileURLToPath(new URL('./updater-manifest.mjs', import.meta.url));

// Made with `tauri signer generate` and `tauri signer sign` over `payload`, so they check this verifier against Tauri's signer.
const payload = Buffer.from('L1 Data updater bundle fixture\n');
const publicKey = 'dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDU5NjYwMkQxRjc0RTg1OEQKUldTTmhVNzMwUUptV2FqTzYwM0FVRzRjOFdEdEdLd1NCbUMxclJKY1NJTm9LNEx3TGMwTG9nWnUK';
const signature = 'dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTTmhVNzMwUUptV2Q4c3ZMQ2k4ODFaY2RoK1hGZTVpWThta3JBNTY5L0doRzBCRnhHTGl0OXRNdTgyYzZ2Q0QyQkpIcUxKTlVrcGVVTklFZzVGWlYyeHlKY0hwTHNWU2dFPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkyNzU0MTMxCWZpbGU6cGF5bG9hZC5iaW4KU0NTemVZS2tCVC9sREJxc3RaRTFaVk4wM1A1WWxDOWlaRDF1Qy9hWUFKME5XUFV2N2ZFc09Vb1I2MGw3Y3JqYWpGSCtjc2tFazNZQWxvZmdGWVBaQ2c9PQo=';
const otherKeySignature = 'dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRbGJxRUcrM1RweUNTekU0Qkd6YWx6ejNQOHpDdlRHSTR6WVZsVHNWQlZFZkhyYlF6Slp6NXZjVnk4RVcxdnQvMVd2R1llNHMwOXNFZERHcHZZQjJKWEhJYUE3U1RUakEwPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkyNzU0MTMxCWZpbGU6cGF5bG9hZC5iaW4KbW5MMERzUWtsa3lVUXNFUzkybGhpRUQwY28yY2NxakZpa3hqS2Z0MWM3bkg4anNDZnptRHE0UDhyZy9RVkVraDZ0akJBNy9QTnpUTzRnR0JDeU5FQnc9PQo=';

const version = '0.1.5-nightly.20261023.42';
const tag = `v${version}`;
const download = name => `https://github.com/ImNallen/L1Data/releases/download/${tag}/${name}`;
const sha256 = bytes => `sha256:${createHash('sha256').update(bytes).digest('hex')}`;

const files = () => ({
  [`L1-Data_${version}_universal.app.tar.gz`]: payload,
  [`L1-Data_${version}_universal.app.tar.gz.sig`]: Buffer.from(`${signature}\n`),
  [`L1-Data_${version}_universal.dmg`]: Buffer.from('dmg'),
  [`L1-Data_${version}_amd64.AppImage`]: payload,
  [`L1-Data_${version}_amd64.AppImage.sig`]: Buffer.from(signature),
  [`L1-Data_${version}_amd64.deb`]: Buffer.from('deb'),
  [`L1-Data_${version}_x64-setup.exe`]: payload,
  [`L1-Data_${version}_x64-setup.exe.sig`]: Buffer.from(signature),
});

const uploaded = (input = files()) =>
  Object.entries(input).map(([name, bytes]) => ({ name, state: 'uploaded', size: bytes.length, digest: sha256(bytes) }));

const build = (overrides = {}) => buildManifest({
  repository: 'ImNallen/L1Data', tag, version, notes: 'notes', pubDate: '2026-10-23T08:30:00.000Z', publicKey, files: files(), assets: uploaded(), ...overrides,
});

test('stages each tauri bundle under a versioned name, with signatures for updater bundles', () => {
  const paths = [
    '/t/release/bundle/macos/L1 Data.app',
    '/t/release/bundle/macos/L1 Data.app.tar.gz',
    '/t/release/bundle/macos/L1 Data.app.tar.gz.sig',
    '/t/release/bundle/dmg/L1 Data_0.1.5-nightly.20261023.42_universal.dmg',
  ];
  assert.deepEqual(stagePlan('macOS', version, paths), [
    { from: '/t/release/bundle/macos/L1 Data.app.tar.gz', to: `L1-Data_${version}_universal.app.tar.gz` },
    { from: '/t/release/bundle/macos/L1 Data.app.tar.gz.sig', to: `L1-Data_${version}_universal.app.tar.gz.sig` },
    { from: '/t/release/bundle/dmg/L1 Data_0.1.5-nightly.20261023.42_universal.dmg', to: `L1-Data_${version}_universal.dmg` },
  ]);
  assert.deepEqual(stagePlan('Windows', version, ['D:\\t\\nsis\\L1 Data_0.1.5_x64-setup.exe', 'D:\\t\\nsis\\L1 Data_0.1.5_x64-setup.exe.sig']), [
    { from: 'D:\\t\\nsis\\L1 Data_0.1.5_x64-setup.exe', to: `L1-Data_${version}_x64-setup.exe` },
    { from: 'D:\\t\\nsis\\L1 Data_0.1.5_x64-setup.exe.sig', to: `L1-Data_${version}_x64-setup.exe.sig` },
  ]);
});

test('staging refuses a missing or ambiguous bundle', () => {
  assert.throws(() => stagePlan('Linux', version, ['/t/a.AppImage', '/t/a.AppImage.sig']), {
    message: 'Expected one .deb bundle on Linux, found 0: ',
  });
  assert.throws(() => stagePlan('Linux', version, ['/t/a.AppImage', '/t/b.AppImage', '/t/a.deb']), {
    message: 'Expected one .AppImage bundle on Linux, found 2: /t/a.AppImage, /t/b.AppImage',
  });
});

test('verifies a Tauri signature and names the key when another key signed', () => {
  verifySignature(publicKey, signature, payload);
  assert.throws(() => verifySignature(publicKey, signature, Buffer.from('tampered')), { message: 'signature does not match the file' });
  assert.throws(() => verifySignature(publicKey, otherKeySignature, payload), {
    message: 'signed by key C8E974FB06A16E25, but the updater trusts key 596602D1F74E858D',
  });
});

test('builds the feed with one universal macOS download for both darwin keys', () => {
  const sig = { signature };
  assert.deepEqual(build(), {
    version,
    notes: 'notes',
    pub_date: '2026-10-23T08:30:00.000Z',
    platforms: {
      'darwin-aarch64': { ...sig, url: download(`L1-Data_${version}_universal.app.tar.gz`) },
      'darwin-x86_64': { ...sig, url: download(`L1-Data_${version}_universal.app.tar.gz`) },
      'linux-x86_64': { ...sig, url: download(`L1-Data_${version}_amd64.AppImage`) },
      'windows-x86_64': { ...sig, url: download(`L1-Data_${version}_x64-setup.exe`) },
    },
  });
});

test('refuses a bundle that was not built or not fully uploaded', () => {
  const { [`L1-Data_${version}_x64-setup.exe`]: _, ...withoutExe } = files();
  assert.throws(() => build({ files: withoutExe }), { message: `L1-Data_${version}_x64-setup.exe was not built` });
  assert.throws(() => build({ assets: uploaded().filter(a => !a.name.endsWith('.deb')) }), {
    message: `L1-Data_${version}_amd64.deb is not an asset of ${tag}`,
  });
  const partial = uploaded().map(a => (a.name.endsWith('.AppImage') ? { ...a, state: 'starter', size: 4 } : a));
  assert.throws(() => build({ assets: partial }), {
    message: `L1-Data_${version}_amd64.AppImage is not fully uploaded (state starter, size 4 of ${payload.length})`,
  });
  const swapped = uploaded().map(a => (a.name.endsWith('.dmg') ? { ...a, digest: sha256(Buffer.from('gmd')) } : a));
  assert.throws(() => build({ assets: swapped }), { message: `L1-Data_${version}_universal.dmg on ${tag} differs from the built file` });
});

test('refuses an updater bundle with a missing or foreign signature', () => {
  const { [`L1-Data_${version}_amd64.AppImage.sig`]: _, ...unsigned } = files();
  assert.throws(() => build({ files: unsigned }), {
    message: `L1-Data_${version}_amd64.AppImage has no L1-Data_${version}_amd64.AppImage.sig`,
  });
  const foreign = { ...files(), [`L1-Data_${version}_x64-setup.exe.sig`]: Buffer.from(otherKeySignature) };
  assert.throws(() => build({ files: foreign }), {
    message: `L1-Data_${version}_x64-setup.exe: signed by key C8E974FB06A16E25, but the updater trusts key 596602D1F74E858D`,
  });
});

test('CLI stages tauri output and writes a dry-run feed from it', async t => {
  const root = await mkdtemp(join(tmpdir(), 'l1-manifest-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const built = join(root, 'bundle');
  await mkdir(built);
  const tauriNames = {
    'L1 Data.app.tar.gz': payload, 'L1 Data.app.tar.gz.sig': Buffer.from(signature), 'L1 Data_universal.dmg': Buffer.from('dmg'),
    'L1 Data_amd64.AppImage': payload, 'L1 Data_amd64.AppImage.sig': Buffer.from(signature), 'L1 Data_amd64.deb': Buffer.from('deb'),
    'L1 Data_x64-setup.exe': payload, 'L1 Data_x64-setup.exe.sig': Buffer.from(signature),
  };
  for (const [name, bytes] of Object.entries(tauriNames)) await writeFile(join(built, name), bytes);
  const run = (args, env = {}) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', env: { ...process.env, ...env } });
  const assets = join(root, 'assets');
  for (const os of ['macOS', 'Linux', 'Windows']) {
    const paths = Object.keys(tauriNames).map(name => join(built, name));
    const staged = run(['stage', os, version, assets], { ARTIFACT_PATHS: JSON.stringify(paths) });
    assert.equal(staged.status, 0, staged.stderr);
  }
  assert.deepEqual((await readdir(assets)).sort(), Object.keys(files()).sort());

  const out = join(root, 'latest.json');
  const env = { GITHUB_REPOSITORY: 'ImNallen/L1Data', UPDATER_PUBKEY: publicKey };
  const missingKey = run(['manifest', assets, tag, version, '--dry-run', out], { ...env, UPDATER_PUBKEY: '' });
  assert.equal(missingKey.status, 1);
  assert.equal(missingKey.stderr, 'UPDATER_PUBKEY must hold the updater public key\n');

  const result = run(['manifest', assets, tag, version, '--dry-run', out], env);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, `latest.json for ${tag}: 4 platforms point at 3 verified downloads.\n`);
  const written = JSON.parse(await readFile(out, 'utf8'));
  assert.deepEqual({ ...written, pub_date: 'x' }, { ...build({ notes: '' }), pub_date: 'x' });
});
