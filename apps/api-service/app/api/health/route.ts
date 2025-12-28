/**
 * Health Check Endpoint
 * GET /api/health
 *
 * Returns the health status of the API service.
 * Used by MCP Server to verify API connectivity.
 */

import { NextResponse } from "next/server";
import type { HealthResponse } from "@/lib/types";

export async function GET(): Promise<NextResponse<HealthResponse>> {
  const response: HealthResponse = {
    status: "healthy",
    timestamp: new Date().toISOString(),
    version: "0.1.0",
    services: {
      database: true, // Mock: always healthy
      cache: true, // Mock: always healthy
    },
  };

  return NextResponse.json(response);
}
