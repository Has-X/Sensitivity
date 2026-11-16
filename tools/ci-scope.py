"""Fail closed to full CI; only Markdown documentation may skip compilation."""
import os
import pathlib
import subprocess


def documentation(path):
    return path.endswith('.md') and (
        '/' not in path or path.startswith('docs/')
    )


def main():
    base, head = os.environ.get('BASE', ''), os.environ.get('HEAD', '')
    paths = []
    if os.environ.get('EVENT') in ('push', 'pull_request') and base and set(base) != {'0'}:
        paths = subprocess.check_output(
            ['git', 'diff', '--name-only', '-z', base, head]
        ).decode('utf-8').strip('\0').split('\0')
    docs_only = bool(paths) and all(documentation(path) for path in paths)
    if docs_only:
        for name in paths:
            path = pathlib.Path(name)
            if not path.exists():
                continue  # Deleted documentation is a valid change.
            text = path.read_text(encoding='utf-8')
            if any(line.startswith(('<<<<<<< ', '=======', '>>>>>>> ')) for line in text.splitlines()):
                raise ValueError(f'Merge conflict marker in {name}')
        subprocess.run(['git', 'diff', '--check', base, head], check=True)
    print(f'Documentation-only: {docs_only}; changed files: {len(paths)}')
    with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as output:
        output.write(f'docs_only={str(docs_only).lower()}\n')


if __name__ == '__main__':
    main()
