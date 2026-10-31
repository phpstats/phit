# Polyscope workspace preview

Contributor note, moved out of docs/documentation.qmd on 2026-10-31.


polyscope.json connects the workspace lifecycle to two scripts:

- Setup runs scripts/workspace-up: render the saved documentation and link
  docs/_site/ in Laravel Herd at http://phit-{{folder}}.test.
- Opening Preview runs setup again (safe to repeat), then starts Quarto's watcher
  with --no-serve. Herd serves the HTML without reserving a shared localhost port.
- Archive runs scripts/workspace-down to remove this workspace's Herd link.
  Stop Preview to stop its watcher. Cleanup does not delete source or build outputs.

This integration requires Quarto and Herd on PATH. Each workspace gets its own
URL based on its folder name, so multiple workspaces can be browsed concurrently.
The served files are static documentation; the compiled PHP extension does not
need to be loaded into Herd. Polyscope's automatic Expose tunnel is enabled so
the preview can be shared with other devices. The public URL is managed by
Polyscope and is available while its tunnel is running.

For manual setup or cleanup, run `./scripts/workspace-up` or
`./scripts/workspace-down` from the checkout. Opening Preview displays the linked
site and keeps its rendering watcher in Polyscope's preview terminal. Editing a
computational walkthrough can require the execution environment described below;
re-rendering saved results does not establish fresh numerical verification.

