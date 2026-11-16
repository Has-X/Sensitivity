import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'tools/ci-scope.py'


class ScopeTests(unittest.TestCase):
    def check_scope(self, path, content, event='pull_request'):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.check_output(['git', '-C', directory, *args], stderr=subprocess.DEVNULL).decode().strip()
            git('init')
            git('config', 'user.name', 'CI test')
            git('config', 'user.email', 'ci@example.invalid')
            (root / 'README.md').write_text('Initial documentation\n')
            git('add', '.')
            git('commit', '-m', 'Initial')
            base = git('rev-parse', 'HEAD')
            target = root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content, encoding='utf-8')
            git('add', '.')
            git('commit', '-m', 'Change')
            output = root / 'output'
            result = subprocess.run(['python3' if os.name != 'nt' else 'python', str(SCRIPT)],
                cwd=root, env={**os.environ, 'BASE': base, 'HEAD': git('rev-parse', 'HEAD'),
                    'EVENT': event, 'GITHUB_OUTPUT': str(output)}, capture_output=True)
            return result.returncode, output.read_text() if output.exists() else ''

    def test_documentation_is_lightweight(self):
        self.assertEqual(self.check_scope('README.md', 'Updated documentation\n'), (0, 'docs_only=true\n'))

    def test_code_and_workflows_require_full_checks(self):
        for path in ('src/main.rs', '.github/workflows/build.yml', 'docs/example.rs'):
            with self.subTest(path=path):
                self.assertEqual(self.check_scope(path, 'content\n'), (0, 'docs_only=false\n'))

    def test_manual_runs_are_full(self):
        self.assertEqual(self.check_scope('README.md', 'Updated\n', 'workflow_dispatch'), (0, 'docs_only=false\n'))

    def test_bad_documentation_does_not_emit_success(self):
        code, output = self.check_scope('README.md', '<<<<<<< conflict\n')
        self.assertNotEqual(code, 0)
        self.assertEqual(output, '')
