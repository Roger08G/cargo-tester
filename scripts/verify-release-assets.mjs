import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const tag = process.env.GITHUB_REF_NAME;
if (!tag || !/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error("A stable release tag is required.");
const directory = process.argv[2];
if (!directory) throw new Error("An artifact directory is required.");
const gh = (args) => execFileSync("gh", args, { encoding: "utf8", timeout: 60_000, windowsHide: true });
const release = JSON.parse(gh(["release", "view", tag, "--json", "tagName,name,isDraft,isPrerelease,assets"]));
if (release.tagName !== tag || release.isDraft || release.isPrerelease) throw new Error("Unexpected release identity/state.");
for (const name of readdirSync(directory)) {
  const path = join(directory, name);
  const digest = `sha256-${createHash("sha256").update(readFileSync(path)).digest("hex")}`;
  const asset = release.assets.find((item) => item.name === name);
  if (asset) {
    const remoteDigest = asset.digest?.replace("sha256:", "sha256-");
    if (remoteDigest !== digest) throw new Error(`Existing asset differs: ${name}. Published assets are immutable.`);
  } else {
    gh(["release", "upload", tag, path]);
  }
}
if (release.name !== `Production ${tag}`) gh(["release", "edit", tag, "--title", `Production ${tag}`]);
process.stdout.write("Verified release asset identities; uploaded missing assets only.\n");
