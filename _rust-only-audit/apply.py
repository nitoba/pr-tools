from pathlib import Path
import re
import subprocess
root = Path.cwd()
def replace(path, old, new, count=1):
    file=root/path
    text=file.read_text(encoding='utf-8')
    assert text.count(old)==count,(path,text.count(old),old)
    file.write_text(text.replace(old,new),encoding='utf-8',newline='\n')

for path in ['.github/workflows/ci.yml','.github/workflows/release.yml']:
    file=root/path
    text=file.read_text(encoding='utf-8')
    start=text.index('  checks:\n')
    end=text.index('  rust-checks:\n')
    text=text[:start]+text[end:]
    file.write_text(text,encoding='utf-8',newline='\n')
replace('.github/workflows/release.yml','    needs: [checks, rust-checks]\n','    needs: rust-checks\n')
file=root/'.github/workflows/release.yml'
text=file.read_text(encoding='utf-8')
text=re.sub(r'^            dart_(source|asset):.*\n','',text,flags=re.M)
start=text.index('      - name: Setup Dart\n')
end=text.index('      - name: Setup Rust\n',start)
file.write_text(text[:start]+text[end:],encoding='utf-8',newline='\n')
replace('.gitignore','# https://dart.dev/tools/private-files\n# Created by `dart pub`\n.dart_tool/\n','')
replace('lefthook.yml','      glob: "**/*.dart"\n      run: dart format {staged_files}\n      stage_fixed: true\n','      glob: "**/*.rs"\n      run: cargo fmt --manifest-path apps/rust/Cargo.toml -- --check\n')
replace('lefthook.yml','      run: cd apps/dart && dart analyze\n','      run: cargo clippy --manifest-path apps/rust/Cargo.toml --locked --all-targets -- -D clippy::correctness\n')
replace('lefthook.yml','    dart-test:\n      run: cd apps/dart && dart test\n','')
replace('scripts/build-rust.sh','no monorepo `pr-tools`.','no repositório `pr-tools`.')
replace('scripts/build-rust.sh','# Como no Dart, o alvo precisa ser o host atual (sem cross por padrão).','# O alvo precisa ser o host atual (sem cross por padrão).')
replace('scripts/install.sh','# Para instalar a implementação Dart de compatibilidade, use:\n#   PR_TOOLS_FLAVOR=dart bash scripts/install.sh\n#\n','')
replace('scripts/install.sh','#   PR_TOOLS_FLAVOR       implementação: rust (padrão) ou dart\n','')
replace('scripts/install.sh','FLAVOR="${PR_TOOLS_FLAVOR:-rust}"\n\ncase "$FLAVOR" in\n  rust) ASSET_PREFIX=\'prt\' ;;\n  dart) ASSET_PREFIX=\'prt-dart\' ;;\n  *) fail "Implementação inválida: $FLAVOR (use rust ou dart)." ;;\nesac\n','')
for target in ['linux-x64','linux-arm64','macos-arm64']:
    replace('scripts/install.sh',f'ASSET="${{ASSET_PREFIX}}-{target}"',f"ASSET='prt-{target}'")
replace('scripts/install.ps1','# PR_TOOLS_REPOSITORY, PR_TOOLS_INSTALL_DIR, PR_TOOLS_BINARY, PR_TOOLS_GITHUB_TOKEN,\n# PR_TOOLS_FLAVOR (rust, padrão, ou dart).','# PR_TOOLS_REPOSITORY, PR_TOOLS_INSTALL_DIR, PR_TOOLS_BINARY, PR_TOOLS_GITHUB_TOKEN.')
replace('scripts/install.ps1',"$flavor = if ($env:PR_TOOLS_FLAVOR) { $env:PR_TOOLS_FLAVOR } else { 'rust' }\n$assetPrefix = switch ($flavor) {\n  'rust' { 'prt' }\n  'dart' { 'prt-dart' }\n  default { throw \"Implementação inválida: $flavor (use rust ou dart).\" }\n}\n\n",'')
replace('scripts/install.ps1','$assetName = "$assetPrefix-windows-x64.exe"',"$assetName = 'prt-windows-x64.exe'")
replace('README.md','ou usa Genkit com endpoints compatíveis com a API da OpenAI.','ou usa o SDK Rust `aisdk` com endpoints compatíveis com a API da OpenAI.')
replace('README.md','As releases publicam a implementação Rust principal para Linux x64/arm64, macOS arm64 e Windows x64. A implementação Dart continua disponível como compatibilidade.','As releases publicam a implementação Rust para Linux x64/arm64, macOS arm64 e Windows x64.')
f=root/'README.md';text=f.read_text(encoding='utf-8')
start=text.index('## Estrutura do monorepo\n')
end=text.index('## Desenvolvimento\n',start)
text=text[:start]+'''## Estrutura do projeto

A aplicação Rust com Ratatui fica em `apps/rust/`:

```text
apps/
└── rust/   # aplicação prt
```

A raiz contém a configuração do repositório, a automação em `scripts/`,
a documentação e os workflows do GitHub. O comando continua sendo `prt`
e a configuração permanece em `~/.config/pr-tools`.

Os instaladores principais são `scripts/install.sh` e `scripts/install.ps1`.
Os aliases `scripts/install-rust.sh` e `scripts/install-rust.ps1` continuam
disponíveis para compatibilidade com instalações existentes.

'''+text[end:]
f.write_text(text,encoding='utf-8',newline='\n')
replace('README.md','# Dart\ncd apps/dart\ndart pub get\ndart analyze\ndart test\ncd ../..\ndart run scripts/build.dart\n\n# Rust\n','')
replace('README.md','''`dart run scripts/build.dart` gera `apps/dart/dist/prt-<plataforma>` para o
host atual. A plataforma também pode ser informada explicitamente, desde que
corresponda ao host, por exemplo `dart run scripts/build.dart linux-x64`.
A release compila cada binário no runner nativo correspondente. Os executáveis
externos `codex` e `opencode` continuam sendo instalados e autenticados
separadamente na máquina do usuário.

`./scripts/build-rust.sh` gera `apps/rust/dist/prt-rust-<plataforma>` para o
host atual, depois de executar as verificações Rust por padrão. Nas releases,
esse binário também é publicado como `prt-<plataforma>` (nome principal),
enquanto o Dart usa `prt-dart-<plataforma>`.
''','''`./scripts/build-rust.sh` gera `apps/rust/dist/prt-rust-<plataforma>` para o
host atual, depois de executar as verificações Rust por padrão. A plataforma
pode ser informada explicitamente, desde que corresponda ao host, por exemplo
`./scripts/build-rust.sh linux-x64`.

A release compila cada binário no runner nativo correspondente e publica o
mesmo executável como `prt-<plataforma>` (nome principal) e
`prt-rust-<plataforma>` (alias de compatibilidade). No Windows, os nomes têm
a extensão `.exe`.

Os executáveis externos `codex` e `opencode` continuam sendo instalados e
autenticados separadamente na máquina do usuário.
''')
subprocess.run(['git','rm','-r','--','apps/dart','.agents/skills/dart-result-effect-refactor','scripts/build.dart'],check=True)
subprocess.run(['git','add','-A'],check=True)
expected='a915d27ccd93bc511533c9a26f31c854dcdf0d9b'
actual=subprocess.check_output(['git','write-tree'],text=True).strip()
assert actual==expected, f'Candidate tree mismatch: {actual} != {expected}'
assert not subprocess.check_output(['git','diff','--cached','--name-only','--','apps/rust'],text=True).strip()
assert not subprocess.check_output(['git','ls-files','*.dart','**/pubspec.yaml','**/pubspec.lock'],text=True).strip()
subprocess.run(['git','diff','--cached','--check'],check=True)
print('Verified exact candidate tree:',actual)
