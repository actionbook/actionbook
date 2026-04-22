const { getInfo } = require("@changesets/get-github-info");

async function getReleaseLine(changeset, type, options) {
  if (!options || !options.repo) {
    throw new Error(
      'A `repo` option is required — specify in .changeset/config.json: "changelog": ["./changelog.js", { "repo": "owner/name" }]'
    );
  }

  const [firstLine, ...rest] = changeset.summary
    .split("\n")
    .map((line) => line.trimEnd());

  let prInfo;
  if (changeset.commit) {
    prInfo = await getInfo({ repo: options.repo, commit: changeset.commit });
  }

  const prLink =
    prInfo && prInfo.pull !== null && prInfo.pull !== undefined
      ? ` [#${prInfo.pull}](https://github.com/${options.repo}/pull/${prInfo.pull})`
      : "";

  const commitLink = changeset.commit
    ? ` [\`${changeset.commit.slice(0, 7)}\`](https://github.com/${options.repo}/commit/${changeset.commit})`
    : "";

  const continuation = rest.length
    ? "\n" + rest.map((line) => `  ${line}`).join("\n")
    : "";

  return `\n\n-${prLink}${commitLink} ${firstLine}${continuation}`;
}

async function getDependencyReleaseLine(changesets, dependenciesUpdated) {
  if (dependenciesUpdated.length === 0) return "";
  const updated = dependenciesUpdated
    .map((dep) => `  - ${dep.name}@${dep.newVersion}`)
    .join("\n");
  return `- Updated dependencies\n${updated}`;
}

module.exports = { getReleaseLine, getDependencyReleaseLine };
