export function getCSSVariable(variableName: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(variableName).trim()
}

export function getChartColors() {
  return {
    accent: getCSSVariable('--accent'),
    textMuted: getCSSVariable('--text-muted'),
    border: getCSSVariable('--border'),
    danger: getCSSVariable('--danger'),
    edgeDerived: getCSSVariable('--edge-derived'),
    edgeContradicts: getCSSVariable('--edge-contradicts'),
    edgeReplicates: getCSSVariable('--edge-replicates'),
    nodeBounty: getCSSVariable('--node-bounty'),
    nodeFinding: getCSSVariable('--node-finding'),
    nodeHypothesis: getCSSVariable('--node-hypothesis'),
    nodeNegativeResult: getCSSVariable('--node-negative-result'),
    nodeSynthesis: getCSSVariable('--node-synthesis'),
  }
}
