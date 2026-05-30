# How to analyze a bare repository

<!-- Template: Good Docs Project how-to (CC-BY 4.0) — https://github.com/thegooddocsproject/templates -->

## Goal

Run history analysis directly against a bare Git repository — such as a GitLab
backup, a Gitolite mirror, or a `git clone --bare` mirror — without first
checking out a working tree.

## Prerequisites

- `codefang` installed.
- A bare repository on disk (a directory ending in `.git` with no working tree).
- For GitLab backups, enough disk space to extract the `repositories/` tree.

## Steps

1. Point `codefang run` directly at the bare repository path. Codefang uses
   libgit2, which opens bare and non-bare repositories transparently, so no
   clone or checkout is required:

   ```bash
   codefang run /data/backups/repositories/@hashed/ab/cd/abcdef.git \
     -a 'history/*' --format json --silent
   ```

2. To scan a GitLab backup, extract the repositories tree from the tarball:

   ```bash
   tar xf backup.tar -C /data/extract repositories/
   ```

3. Build a list of real project repos, skipping wiki and design repos:

   ```bash
   find /data/extract/repositories -name "*.git" \
     ! -name "*.wiki.git" \
     ! -name "*.design.git" \
     -type d > /tmp/repo_paths.txt
   ```

4. Run a single bare repo with a memory budget so the streaming pipeline stays
   bounded on large histories:

   ```bash
   codefang run /data/extract/repositories/@hashed/ab/cd/abcdef.git \
     -a 'history/*' --memory-budget 4GiB --workers 4 --format json --silent
   ```

5. For periodic scans, analyze only new commits with `--since` rather than
   re-walking the whole history each run:

   ```bash
   codefang run /data/extract/repositories/@hashed/ab/cd/abcdef.git \
     -a 'history/*' --since 168h --format json --silent
   ```

## Result

You get the same history report you would get from a normal working clone —
burndown, developers, couples, and more — produced straight from the bare repo
with no checkout. A non-empty JSON report on stdout confirms the bare repo was
opened and analyzed.

## See also

- [Large-scale repository scanning](../operations/large-scale-scanning.md)
- [How to analyze a monorepo](analyze-a-monorepo.md)
- [CLI reference](../guide/cli-reference.md)
