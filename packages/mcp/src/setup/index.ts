export { agentSetup, parseSetupArgs } from './agent-setup.js'
export { AgentSetupInputSchema, SetupTargetSchema } from './types.js'
export type {
  AgentSetupInput,
  AgentSetupResult,
  FileResult,
  SetupTarget,
} from './types.js'
export {
  buildMcpServerEntry,
  generateMcpConfig,
  generateClaudeCodePermissions,
  generateEnvContent,
  getMcpConfigPath,
  getPermissionsConfigPath,
} from './generators.js'
