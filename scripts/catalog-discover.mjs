#!/usr/bin/env node
// Asks Hugging Face which model files really exist (Q4_K_M quantizations and vision projectors),
// with their size, sha256 and license, and GitHub for the llama.cpp engine archives' sha256.
// Runs in CI (the developer machine may have no access): Actions → catalog → Run workflow.
// Output: one JSON document on stdout, used to pin src-tauri/catalog.json by hand.
const ENGINE_TAG = process.env.ENGINE_TAG || "b11351";
const REPOS = (process.env.REPOS || [
  "unsloth/Qwen3.5-0.8B-GGUF",
  "unsloth/Qwen3.5-2B-GGUF",
  "unsloth/Qwen3.5-4B-GGUF",
  "unsloth/Qwen3.5-9B-GGUF",
  "unsloth/Qwen3.5-27B-GGUF",
  "unsloth/Qwen3.5-35B-A3B-GGUF",
  "unsloth/Qwen3.6-35B-A3B-GGUF",
  "unsloth/Qwen3.8-27B-GGUF",
  "unsloth/Qwen3-VL-4B-Instruct-GGUF",
  "unsloth/Qwen3-VL-8B-Instruct-GGUF",
  "unsloth/gemma-3-4b-it-GGUF",
  "unsloth/gemma-3-12b-it-GGUF",
  "unsloth/gemma-3-27b-it-GGUF",
].join(",")).split(",");

const WANTED = /(^|[-_/])(UD-)?Q4_K_M\.gguf$|mmproj.*\.gguf$/i;

async function hf(repo) {
  const r = await fetch(`https://huggingface.co/api/models/${repo}?blobs=true`);
  if (!r.ok) return { repo, error: `HTTP ${r.status}` };
  const j = await r.json();
  const files = (j.siblings || [])
    .filter((s) => WANTED.test(s.rfilename) && !s.rfilename.includes("/"))
    .map((s) => ({ file: s.rfilename, size: s.size ?? s.lfs?.size ?? null, sha256: s.lfs?.sha256 ?? null }));
  return {
    repo,
    license: j.cardData?.license ?? (j.tags || []).find((t) => t.startsWith("license:"))?.slice(8) ?? null,
    base: j.cardData?.base_model ?? null,
    pipeline: j.pipeline_tag ?? null,
    files,
  };
}

async function engine() {
  const headers = { Accept: "application/vnd.github+json" };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const r = await fetch(`https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/${ENGINE_TAG}`, { headers });
  if (!r.ok) return { tag: ENGINE_TAG, error: `HTTP ${r.status}` };
  const j = await r.json();
  const want = /bin-(ubuntu-(vulkan-)?(x64|arm64)\.tar\.gz|macos-arm64\.tar\.gz|win-(cpu|vulkan)-x64\.zip)$/;
  return {
    tag: ENGINE_TAG,
    assets: j.assets.filter((a) => want.test(a.name)).map((a) => ({ name: a.name, size: a.size, sha256: (a.digest || "").replace(/^sha256:/, "") || null })),
  };
}

const out = { engine: await engine(), models: [] };
for (const repo of REPOS) out.models.push(await hf(repo.trim()));
console.log(JSON.stringify(out, null, 2));
