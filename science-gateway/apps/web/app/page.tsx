import { HomePageClient, type ModelOption, type ExecutionTargetOption } from "@/components/home-page-client";
import { prisma } from "@/lib/prisma";

function parseCapabilities(capabilitiesJson: string) {
  const parsedValue: unknown = JSON.parse(capabilitiesJson);

  return Array.isArray(parsedValue)
    ? parsedValue.filter((capability): capability is string => typeof capability === "string")
    : [];
}

export default async function HomePage() {
  const modelBackends = await prisma.modelBackend.findMany({
    orderBy: { id: "asc" }
  });

  const executionTargets = await prisma.executionTarget.findMany({
    orderBy: { id: "asc" }
  });

  const modelOptions: ModelOption[] = modelBackends.map((modelBackend) => ({
    slug: modelBackend.slug,
    label: modelBackend.label,
    summary: modelBackend.summary,
    capabilities: parseCapabilities(modelBackend.capabilitiesJson)
  }));

  const executionTargetOptions: ExecutionTargetOption[] = executionTargets.map((executionTarget) => ({
    slug: executionTarget.slug,
    label: executionTarget.label,
    summary: executionTarget.summary,
    kind: executionTarget.kind,
    status: executionTarget.status as "available" | "planned" | "disabled"
  }));

  return <HomePageClient modelOptions={modelOptions} executionTargets={executionTargetOptions} />;
}

