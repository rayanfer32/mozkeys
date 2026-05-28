Ran command: `bun run serve`
Viewed release.yml:1-22

To push a version tag (e.g., `v1.0.0`) to trigger the GitHub Release workflow, run the following Git commands in your terminal:

### 1. Commit your changes
Ensure all your current code changes are committed first:
```bash
git add .
git commit -m "Prepare release v1.0.0"
```

### 2. Create the tag locally
Create a lightweight tag pointing to your latest commit:
```bash
git tag v1.0.0
```

### 3. Push the tag to GitHub
Push the tag to your remote repository on GitHub (`origin`):
```bash
git push origin v1.0.0
```

Once pushed, GitHub will automatically detect the `v1.0.0` tag, kick off the release action on a Windows environment, compile the project, and create a release under that tag containing the `mozkeys.exe` executable!