"""Exercise real Bash installers with isolated HOME and stubbed downloads/platforms."""
from pathlib import Path
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(os.environ.get('REPO_ROOT', '.'))
FIXTURE = '#!/usr/bin/env bash\nprintf "prt installer-test-fixture\\n"\n'

class Installers(unittest.TestCase):
    def run_install(self, *, script='install.sh', system='Linux', arch='x86_64',
                    flavor=None, version='4.0.11', local=False, twice=False):
        with tempfile.TemporaryDirectory(prefix='prt-installer-') as tmp:
            root = Path(tmp)
            home, tools, dest = root/'home', root/'tools', root/'bin with spaces'
            home.mkdir(); tools.mkdir()
            (tools/'uname').write_text('#!/bin/bash\ncase "$1" in -s) echo "$TEST_OS";; -m) echo "$TEST_ARCH";; *) exit 2;; esac\n')
            (tools/'curl').write_text(f'''#!{sys.executable}
import json, os, pathlib, sys
args = sys.argv[1:]
url = next(a for a in args if a.startswith('https://'))
with open(os.environ['CURL_LOG'], 'a') as f: f.write(json.dumps(url)+'\\n')
out = args[args.index('--output')+1]
if out == '/dev/null':
    print(url, end='')
else:
    pathlib.Path(out).write_text({FIXTURE!r})
''')
            for tool in tools.iterdir(): tool.chmod(0o755)
            env = {k:v for k,v in os.environ.items() if not k.startswith(('PR_TOOLS_', 'XDG_'))}
            env.update(HOME=str(home), SHELL='/bin/bash', PATH=f'{tools}:/usr/local/bin:/usr/bin:/bin',
                       TEST_OS=system, TEST_ARCH=arch, CURL_LOG=str(root/'curl.jsonl'),
                       PR_TOOLS_VERSION=version, PR_TOOLS_INSTALL_DIR=str(dest), NO_COLOR='1')
            if flavor is not None: env['PR_TOOLS_FLAVOR']=flavor
            if local:
                source=root/'local-prt'; source.write_text(FIXTURE); source.chmod(0o755)
                env['PR_TOOLS_BINARY']=str(source)
            results=[]
            for _ in range(2 if twice else 1):
                results.append(subprocess.run(['bash', str(ROOT/'scripts'/script), '--yes'], env=env,
                                              text=True, capture_output=True, timeout=15))
            installed=dest/'prt'
            downloads=[json.loads(s) for s in (root/'curl.jsonl').read_text().splitlines()] if (root/'curl.jsonl').exists() else []
            return dict(results=results, urls=downloads, exists=installed.exists(),
                        body=installed.read_text() if installed.exists() else None,
                        mode=stat.S_IMODE(installed.stat().st_mode) if installed.exists() else None,
                        profile=(home/'.profile').read_text() if (home/'.profile').exists() else '',
                        bashrc=(home/'.bashrc').read_text() if (home/'.bashrc').exists() else '')

    def assert_installed(self, result):
        for run in result['results']:
            self.assertEqual(run.returncode, 0, run.stdout+run.stderr)
        self.assertEqual(result['body'], FIXTURE)
        self.assertEqual(result['mode'], 0o755)

    def test_primary_assets_for_all_unix_platforms(self):
        for system,arch,asset in [('Linux','x86_64','linux-x64'),('Linux','arm64','linux-arm64'),('Darwin','arm64','macos-arm64')]:
            with self.subTest(platform=asset):
                r=self.run_install(system=system,arch=arch)
                self.assert_installed(r)
                self.assertEqual(r['urls'], [f'https://github.com/nitoba/pr-tools/releases/download/v4.0.11/prt-{asset}'])

    def test_legacy_flavor_environment_cannot_select_another_implementation(self):
        for flavor in ['rust','dart','other']:
            with self.subTest(flavor=flavor):
                r=self.run_install(flavor=flavor)
                self.assert_installed(r)
                self.assertEqual(r['urls'], ['https://github.com/nitoba/pr-tools/releases/download/v4.0.11/prt-linux-x64'])

    def test_latest_and_prefixed_versions(self):
        for version,segment in [('latest','latest/download'),('v4.0.11','download/v4.0.11')]:
            with self.subTest(version=version):
                r=self.run_install(version=version)
                self.assert_installed(r)
                self.assertTrue(r['urls'])
                self.assertEqual(set(r['urls']), {f'https://github.com/nitoba/pr-tools/releases/{segment}/prt-linux-x64'})

    def test_rust_alias_assets_are_preserved(self):
        for system,arch,asset in [('Linux','x86_64','linux-x64'),('Linux','aarch64','linux-arm64'),('Darwin','arm64','macos-arm64')]:
            with self.subTest(platform=asset):
                r=self.run_install(script='install-rust.sh',system=system,arch=arch,flavor='dart')
                self.assert_installed(r)
                self.assertEqual(r['urls'], [f'https://github.com/nitoba/pr-tools/releases/download/v4.0.11/prt-rust-{asset}'])

    def test_local_binary_does_not_download(self):
        r=self.run_install(local=True,flavor='dart')
        self.assert_installed(r)
        self.assertEqual(r['urls'], [])

    def test_path_updates_are_idempotent(self):
        r=self.run_install(local=True,twice=True)
        self.assert_installed(r)
        self.assertEqual(r['profile'].count('# Added by prt installer'),1)
        self.assertEqual(r['bashrc'].count('# Added by prt installer'),1)

    def test_unsupported_platform_fails_before_download(self):
        for system,arch in [('Linux','riscv64'),('Darwin','x86_64'),('MINGW64_NT','x86_64')]:
            with self.subTest(system=system,arch=arch):
                r=self.run_install(system=system,arch=arch)
                self.assertNotEqual(r['results'][0].returncode,0)
                self.assertFalse(r['exists'])
                self.assertEqual(r['urls'], [])

if __name__ == '__main__': unittest.main(verbosity=2)
