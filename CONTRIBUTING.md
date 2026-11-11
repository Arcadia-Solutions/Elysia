# Contributing

Thanks for wanting to help! Bug reports, ideas and pull requests are all welcome.

## Before you start

- For anything bigger than a small fix, open an issue or say hi on [Discord](https://discord.gg/amYWVk7pS3) / [Matrix](https://matrix.to/#/%23Arcadia-Solutions%3Amatrix.org?via=matrix.org) first, so we can agree on the approach before you spend time on it.
- Read the [AI policy](./AI_POLICY.md) if you use AI tools.

## Setup

The [development docs](https://arcadia-solutions.github.io/Elysia/development/getting-started.html) cover running the backend and frontend locally, test data, database changes and regenerating the API client.

## Pull requests

- Keep them focused: one change per PR.
- Add or update tests for what you change (see [Testing](https://arcadia-solutions.github.io/Elysia/development/testing.html)).
- Update the docs in `docs/` when behavior or configuration changes.
- After changing the API, regenerate the frontend client (`npm run gen:api`), CI fails on schema drift.
- Make sure the checks pass:

  ```sh
  # backend
  cargo clippy
  cargo fmt --all -- --check
  cargo test
  # frontend
  npm run lint
  npm run type-check
  npm run check-format
  ```

- Use [Conventional Commits](https://www.conventionalcommits.org/)

## License

By contributing, you agree that your contributions are licensed under the [AGPL-3.0](./LICENSE).
