import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const run = (cwd, command, args) => execFileSync(command, args, {cwd, maxBuffer: 256 * 1024 * 1024, stdio: ['ignore', 'pipe', 'inherit']}).toString().trim();
const git = (cwd, ...args) => run(cwd, 'git', args);
const die = (message) => { throw new Error(message); };
const sha = /^[0-9a-f]{40}$/;
const namePattern = /^[a-z][a-z0-9-]*$/;
const mode = process.argv[2];
if (!['--check', '--publish'].includes(mode) || process.argv.length !== 3) die('Usage: node scripts/publish.mjs --check|--publish');
const config = JSON.parse(readFileSync(join(root, 'packages.json'), 'utf8'));
const scratch = mkdtempSync(join(tmpdir(), 'cargo-patches-'));
try {
  const remote = new Map();
  if (mode === '--publish') {
    for (const line of git(root, 'ls-remote', '--refs', 'origin').split('\n').filter(Boolean)) {
      const [hash, ref] = line.split('\t');
      remote.set(ref, hash);
    }
  }
  const staging = join(scratch, 'staging');
  mkdirSync(staging);
  git(staging, 'init', '--quiet');
  git(staging, 'config', 'user.name', 'cargo-patches publisher');
  git(staging, 'config', 'user.email', 'cargo-patches@users.noreply.github.com');
  const pushes = [];
  for (const [family, packages] of Object.entries(config)) {
    if (!namePattern.test(family) || !Array.isArray(packages) || !packages.length) die('Invalid family: ' + family);
    for (const entry of readdirSync(staging)) if (entry !== '.git') rmSync(join(staging, entry), {recursive: true, force: true});
    const names = new Set();
    for (const pkg of packages) {
      if (!namePattern.test(pkg.name) || names.has(pkg.name) || !sha.test(pkg.rev) || !/^https:\/\/github\.com\/[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+\.git$/.test(pkg.url) || pkg.patch !== 'patches/' + pkg.name + '.patch') die('Invalid package: ' + JSON.stringify(pkg));
      names.add(pkg.name);
      const source = join(scratch, 'source-' + family + '-' + pkg.name);
      mkdirSync(source);
      git(source, 'init', '--quiet');
      git(source, 'fetch', '--quiet', '--depth=1', '--no-tags', pkg.url, pkg.rev);
      if (git(source, 'rev-parse', 'FETCH_HEAD') !== pkg.rev) die('Source revision mismatch');
      git(source, 'checkout', '--quiet', '--detach', 'FETCH_HEAD');
      const patch = resolve(root, pkg.patch);
      git(source, 'apply', '--check', patch);
      git(source, 'apply', patch);
      const manifest = join(source, 'Cargo.toml');
      let contents = readFileSync(manifest, 'utf8');
      for (const replacement of pkg.replacements ?? []) {
        const count = contents.split(replacement.from).length - 1;
        if (!replacement.from || count !== replacement.count) die('Manifest replacement count mismatch: ' + pkg.name);
        contents = contents.replaceAll(replacement.from, replacement.to);
      }
      writeFileSync(manifest, contents);
      if (!new RegExp('^name = "' + pkg.name + '"$', 'm').test(contents)) die('Package name mismatch: ' + pkg.name);
      rmSync(join(source, '.git'), {recursive: true});
      // Workflows are source metadata in nested crates, never root workflows.
      cpSync(source, join(staging, pkg.name), {recursive: true, verbatimSymlinks: true});
      console.log(family + ': ' + pkg.name + ' @ ' + pkg.rev + ' applied');
    }
    writeFileSync(join(staging, 'provenance.json'), JSON.stringify({family, packages}, null, 2) + '\n');
    git(staging, 'add', '--force', '--all');
    const tree = git(staging, 'write-tree');
    if (mode === '--check') continue;
    const branch = 'refs/heads/generated/' + family;
    const previous = remote.get(branch);
    if (previous) {
      git(root, 'fetch', '--quiet', '--no-tags', 'origin', branch);
      if (git(root, 'rev-parse', 'FETCH_HEAD') !== previous) die('Concurrent publication; retry');
      git(staging, 'fetch', '--quiet', '--no-tags', root, previous);
      if (git(staging, 'rev-parse', previous + '^{tree}') === tree) {
        console.log(family + ': unchanged');
        continue;
      }
    }
    const prefix = 'refs/tags/' + family + '-v0.0.';
    const versions = [...remote.keys()].filter(ref => ref.startsWith(prefix)).map(ref => Number(ref.slice(prefix.length)));
    if (versions.some(v => !Number.isSafeInteger(v) || v < 1)) die('Invalid publication tag');
    const tag = prefix + (Math.max(0, ...versions) + 1);
    const args = ['commit-tree', tree, ...(previous ? ['-p', previous] : []), '-m', 'Publish ' + family + '\n\nPatch source: ' + git(root, 'rev-parse', 'HEAD')];
    const commit = git(staging, ...args);
    git(root, 'fetch', '--quiet', '--no-tags', staging, commit);
    pushes.push(commit + ':' + branch, commit + ':' + tag);
    console.log(family + ': ' + commit + ' -> ' + tag);
  }
  if (pushes.length) git(root, 'push', '--atomic', 'origin', ...pushes);
} finally {
  rmSync(scratch, {recursive: true, force: true});
}
