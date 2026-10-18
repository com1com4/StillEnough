import json, os, pathlib, subprocess, sys
sys.path.insert(0, str(pathlib.Path('output/qa-deps').resolve()))
from jsonschema import Draft202012Validator, FormatChecker
schema=json.loads(pathlib.Path('docs/cli-response-v1.schema.json').read_text(encoding='utf-8'))
Draft202012Validator.check_schema(schema)
validator=Draft202012Validator(schema, format_checker=FormatChecker())
exe=pathlib.Path('target/debug/stillenough-cli.exe').resolve()
for args,code in [(['version','--json'],0),(['providers','--json'],0),(['providers','--provider','codex','--json'],0),(['--json','version','--bad','SECRET_SENTINEL'],2)]:
    result=subprocess.run([str(exe),*args],capture_output=True,env={**os.environ,'OPENAI_API_KEY':'SECRET_SENTINEL','CODEX_HOME':'SECRET_SENTINEL'})
    assert result.returncode==code
    assert not result.stderr and b'SECRET_SENTINEL' not in result.stdout
    assert not result.stdout.startswith(b'\xef\xbb\xbf')
    validator.validate(json.loads(result.stdout))
    assert result.stdout.endswith(b'\n')
result=subprocess.run([str(exe),'providers','--no-color'],capture_output=True,check=True)
text=result.stdout.decode('utf-8'); assert '阿里百炼' in text and '\x1b' not in text
print('JSON Schema 2020-12 / UTF-8 / secret sentinel: passed')

for path in pathlib.Path('output/cli-schema-cases').glob('*.json'):
    validator.validate(json.loads(path.read_bytes()))
    print(f'Schema example: {path.name} passed')
