import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();

const modelBackends = [
  {
    slug: "openfold",
    label: "OpenFold",
    summary: "Research-oriented folding workflow with a familiar AlphaFold-style pipeline.",
    capabilitiesJson: JSON.stringify([
      "Strong baseline for canonical protein structure prediction",
      "Good fit for future batch job orchestration",
      "Natural candidate for adapter-backed local or remote execution"
    ])
  },
  {
    slug: "boltz",
    label: "Boltz",
    summary: "Fast iteration path for trying alternate folding backends in a shared workbench.",
    capabilitiesJson: JSON.stringify([
      "Useful for comparing outputs across model families",
      "Good target for lightweight adapter integration experiments",
      "Supports future run metadata and benchmarking views"
    ])
  },
  {
    slug: "esmfold",
    label: "ESMFold",
    summary: "Sequence-to-structure model with a streamlined path from raw input to prediction.",
    capabilitiesJson: JSON.stringify([
      "Simple user flow for quick prototype submissions",
      "Potentially strong default for short interactive runs",
      "Easy to position alongside other models in a unified run history"
    ])
  }
];

const executionTargets = [
  {
    slug: "local-mock",
    label: "Local mock",
    summary: "Returns static sample outputs for UI and architecture validation.",
    kind: "local_mock",
    status: "available",
    configJson: "{}",
  },
  {
    slug: "local-runtime",
    label: "Local runtime",
    summary: "Runs adapters on the local machine. Planned integration.",
    kind: "local_runtime",
    status: "planned",
    configJson: "{}",
  },
  {
    slug: "pace-hpc",
    label: "PACE / HPC",
    summary: "Submits jobs to the PACE cluster. Planned integration.",
    kind: "hpc",
    status: "planned",
    configJson: "{}",
  },
  {
    slug: "science-gateway",
    label: "Science Gateway",
    summary: "Executes through a deployed remote science gateway.",
    kind: "gateway",
    status: "planned",
    configJson: "{}",
  },
];

async function main() {
  for (const modelBackend of modelBackends) {
    await prisma.modelBackend.upsert({
      where: { slug: modelBackend.slug },
      update: modelBackend,
      create: modelBackend
    });
  }
    for (const executionTarget of executionTargets) {
    await prisma.executionTarget.upsert({
      where: { slug: executionTarget.slug },
      update: executionTarget,
      create: executionTarget
    });
  }
}

main()
  .catch(async (error) => {
    console.error(error);
    process.exitCode = 1;
  })
  .finally(async () => {
    await prisma.$disconnect();
  });
