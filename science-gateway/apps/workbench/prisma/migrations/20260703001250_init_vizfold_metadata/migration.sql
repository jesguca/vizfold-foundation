-- CreateTable
CREATE TABLE "ModelBackend" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "slug" TEXT NOT NULL,
    "label" TEXT NOT NULL,
    "summary" TEXT NOT NULL,
    "capabilitiesJson" TEXT NOT NULL,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL
);

-- CreateTable
CREATE TABLE "ExecutionTarget" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "slug" TEXT NOT NULL,
    "label" TEXT NOT NULL,
    "summary" TEXT NOT NULL,
    "kind" TEXT NOT NULL,
    "status" TEXT NOT NULL,
    "configJson" TEXT,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL
);

-- CreateTable
CREATE TABLE "Run" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "jobName" TEXT NOT NULL,
    "inputText" TEXT NOT NULL,
    "status" TEXT NOT NULL,
    "outputJson" TEXT,
    "modelBackendId" INTEGER NOT NULL,
    "executionTargetId" INTEGER NOT NULL,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL,
    CONSTRAINT "Run_modelBackendId_fkey" FOREIGN KEY ("modelBackendId") REFERENCES "ModelBackend" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
    CONSTRAINT "Run_executionTargetId_fkey" FOREIGN KEY ("executionTargetId") REFERENCES "ExecutionTarget" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

-- CreateIndex
CREATE UNIQUE INDEX "ModelBackend_slug_key" ON "ModelBackend"("slug");

-- CreateIndex
CREATE UNIQUE INDEX "ExecutionTarget_slug_key" ON "ExecutionTarget"("slug");
