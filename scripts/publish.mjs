import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, relative, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const run = (cwd, command, args) => execFileSync(command, args, {cwd, maxBuffer: 256 * 1024 * 1024, stdio: ['ignore', 'pipe', 'inherit']}).toString().trim();
const git = (cwd, ...args) => run(cwd, 'git', args);
const die = (message) => { throw new Error(message); };
const sha = /^[0-9a-f]{40}$/;
const namePattern = /^[a-z][a-z0-9-]*$/;
const packagePattern = /^[a-z][a-z0-9_-]*$/;
const mode = process.argv[2];
if (!['--check', '--publish'].includes(mode) || process.argv.length !== 3) die('Usage: node scripts/publish.mjs --check|--publish');
const config = JSON.parse(readFileSync(join(root, 'packages.json'), 'utf8'));
const scratch = mkdtempSync(join(tmpdir(), 'cargo-patches-'));
try {
  const remote = new Map();
  const sourceCommit = git(root, 'rev-parse', 'HEAD');
  if (mode === '--publish') {
    for (const line of git(root, 'ls-remote', '--refs', 'origin').split('\n').filter(Boolean)) {
      const [hash, ref] = line.split('\t');
      remote.set(ref, hash);
    }
  }
  if (mode === '--publish') {
    if (git(root, 'status', '--porcelain')) die('Publish requires a clean checkout');
    if (remote.get('refs/heads/main') !== sourceCommit) {
      console.log('Skipping stale or non-main patch queue revision');
      rmSync(scratch, {recursive: true, force: true});
      process.exit(0);
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
      if (!packagePattern.test(pkg.name) || names.has(pkg.name) || pkg.patch !== 'patches/' + pkg.name + '.patch') die('Invalid package: ' + JSON.stringify(pkg));
      names.add(pkg.name);
      const source = join(scratch, 'source-' + family + '-' + pkg.name);
      mkdirSync(source);
      let revision;
      if (pkg.version !== undefined) {
        if (!/^\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?$/.test(pkg.version) || !/^[a-f0-9]{64}$/.test(pkg.checksum) || pkg.url !== undefined || pkg.rev !== undefined) die('Invalid registry source');
        const archive = join(scratch, family + '-' + pkg.name + '.crate');
        const stem = pkg.name + '-' + pkg.version;
        run(scratch, 'curl', ['--fail', '--silent', '--show-error', '--location', '--retry', '3', 'https://static.crates.io/crates/' + pkg.name + '/' + stem + '.crate', '--output', archive]);
        const actual = createHash('sha256').update(readFileSync(archive)).digest('hex');
        if (actual !== pkg.checksum) die('Registry archive checksum mismatch: ' + pkg.name);
        run(source, 'tar', ['-xf', archive, '--strip-components=1', '--no-same-owner', '--no-same-permissions']);
        revision = pkg.version + ' (sha256 ' + actual + ')';
      } else {
        if (!sha.test(pkg.rev) || !/^https:\/\/github\.com\/[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+\.git$/.test(pkg.url) || pkg.checksum !== undefined) die('Invalid Git source');
        git(source, 'init', '--quiet');
        git(source, 'fetch', '--quiet', '--depth=1', '--no-tags', pkg.url, pkg.rev);
        if (git(source, 'rev-parse', 'FETCH_HEAD') !== pkg.rev) die('Source revision mismatch');
        git(source, 'checkout', '--quiet', '--detach', 'FETCH_HEAD');
        if (git(source, 'ls-files', '--stage').split('\n').some(line => line.startsWith('160000 '))) die('Git submodules require an explicit import policy: ' + pkg.name);
        revision = pkg.rev;
      }
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
      if (pkg.workspace_packages !== undefined) {
        if (pkg.version !== undefined || !Array.isArray(pkg.workspace_packages) || !pkg.workspace_packages.length) die('Invalid workspace source: ' + pkg.name);
        const members = new Set();
        for (const member of pkg.workspace_packages) {
          if (!packagePattern.test(member.name) || members.has(member.name) || typeof member.manifest !== 'string' || !member.manifest.endsWith('/Cargo.toml') || member.manifest.split('/').some(part => !part || part === '.' || part === '..' || part.includes('\\'))) die('Invalid workspace member: ' + pkg.name);
          members.add(member.name);
        }
        if (!members.has(pkg.name)) die('Workspace import name must identify a member: ' + pkg.name);
      } else if (!new RegExp('^name = "' + pkg.name + '"$', 'm').test(contents)) die('Package name mismatch: ' + pkg.name);
      // Exclude explicitly identified upstream fixtures from the published
      // source, after verifying the exact bytes. Never copy them into Git.
      for (const exclusion of pkg.exclude_files ?? []) {
        const segments = exclusion.path.split('/');
        if (isAbsolute(exclusion.path) || segments.some(part => !part || part === '.' || part === '..' || part.includes('\\')) || !/^[a-f0-9]{64}$/.test(exclusion.checksum)) die('Invalid exclusion: ' + pkg.name);
        const file = resolve(source, exclusion.path);
        if (relative(source, file) !== exclusion.path) die('Invalid exclusion path: ' + pkg.name);
        const actual = createHash('sha256').update(readFileSync(file)).digest('hex');
        if (actual !== exclusion.checksum) die('Excluded file checksum mismatch: ' + pkg.name);
        rmSync(file);
        console.log(pkg.name + ': excluded upstream fixture ' + exclusion.path);
      }
      rmSync(join(source, '.git'), {recursive: true, force: true});
      // Workflows are source metadata in nested crates, never root workflows.
      cpSync(source, join(staging, pkg.name), {recursive: true, verbatimSymlinks: true});
      console.log(family + ': ' + pkg.name + ' @ ' + revision + ' applied');
    }
    writeFileSync(join(staging, 'provenance.json'), JSON.stringify({family, packages}, null, 2) + '\n');
    git(staging, 'add', '--force', '--all');
    const tree = git(staging, 'write-tree');
    if (mode === '--check') {
      for (const pkg of packages) {
        const manifest = join(staging, pkg.name, 'Cargo.toml');
        const metadata = JSON.parse(run(staging, 'cargo', ['metadata', '--no-deps', '--format-version', '1', '--manifest-path', manifest]));
        if (pkg.workspace_packages !== undefined) {
          const workspace = join(staging, pkg.name);
          if (metadata.workspace_root !== workspace) die('Unexpected workspace root: ' + pkg.name);
          for (const member of pkg.workspace_packages) {
            if (!metadata.packages.some(p => p.name === member.name && p.manifest_path === join(workspace, member.manifest) && metadata.workspace_members.includes(p.id))) die('Workspace package not discoverable: ' + member.name);
          }
        } else if (!metadata.packages.some(p => p.name === pkg.name && p.manifest_path === manifest)) die('Cargo package not discoverable: ' + pkg.name);
      }
      const validation = join(root, 'validation', family);
      if (existsSync(validation)) {
        // A validation workspace must not enclose an imported workspace:
        // Cargo otherwise resolves inherited member fields against the parent.
        const validationRoot = packages.some(pkg => pkg.workspace_packages !== undefined)
          ? join(scratch, 'validation-' + family) : staging;
        mkdirSync(validationRoot, {recursive: true});
        for (const file of ['Cargo.toml', 'Cargo.lock']) {
          const destination = join(validationRoot, file);
          if (existsSync(destination)) die('Validation workspace would overwrite imported content: ' + family);
          cpSync(join(validation, file), destination);
        }
        console.log(family + ': running checked family validation');
        execFileSync('bash', [join(validation, 'test.sh')], {cwd: validationRoot, stdio: 'inherit'});
      }
      continue;
    }
    const branch = 'refs/heads/generated/' + family;
    const previous = remote.get(branch);
    const prefix = 'refs/tags/' + family + '-v0.0.';
    const versions = [...remote.keys()].filter(ref => ref.startsWith(prefix)).map(ref => Number(ref.slice(prefix.length)));
    if (versions.some(v => !Number.isSafeInteger(v) || v < 1)) die('Invalid publication tag');
    const latest = Math.max(0, ...versions);
    if (Boolean(previous) !== Boolean(latest) || (previous && remote.get(prefix + latest) !== previous)) die('Publication branch/tag mismatch: ' + family);
    if (previous) {
      git(root, 'fetch', '--quiet', '--no-tags', 'origin', branch);
      if (git(root, 'rev-parse', 'FETCH_HEAD') !== previous) die('Concurrent publication; retry');
      git(staging, 'fetch', '--quiet', '--no-tags', root, previous);
      if (git(staging, 'rev-parse', previous + '^{tree}') === tree) {
        console.log(family + ': unchanged');
        continue;
      }
    }
    const tag = prefix + (latest + 1);
    const args = ['commit-tree', tree, ...(previous ? ['-p', previous] : []), '-m', 'Publish ' + family + '\n\nPatch source: ' + sourceCommit];
    const commit = git(staging, ...args);
    git(root, 'fetch', '--quiet', '--no-tags', staging, commit);
    pushes.push(commit + ':' + branch, commit + ':' + tag);
    console.log(family + ': ' + commit + ' -> ' + tag);
  }
  if (pushes.length) {
    if (git(root, 'ls-remote', '--refs', 'origin', 'refs/heads/main').split('\t')[0] !== sourceCommit) {
      console.log('Main changed during preparation; newer CI run will publish');
    } else {
      git(root, 'push', '--atomic', 'origin', ...pushes);
    }
  }
} finally {
  rmSync(scratch, {recursive: true, force: true});
}
