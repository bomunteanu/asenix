import type { Atom } from '#/lib/bindings'

export interface ProcessedAtom {
  atom_id: string
  atom_type: string
  lifecycle: string
  conditions: any
  metrics: any
  val_accuracy?: number
  val_loss?: number
  optimizer?: string
  scheduler?: string
  learning_rate?: number
  time_index: number
}

export interface DashboardStats {
  totalAtoms: number
  trainingRuns: number
  contradictions: number
  bounties: number
  hypotheses: number
}

export interface TopRun {
  rank: number
  val_accuracy: number
  val_loss?: number
  optimizer: string
  scheduler?: string
  learning_rate?: number
  lifecycle: string
  atom_id: string
}

export interface BestAccuracyPoint {
  time_index: number
  best_accuracy: number
}

export function processAtoms(atoms: Atom[]): ProcessedAtom[] {
  return atoms.map((atom, index) => {
    const valAccuracyMetric = atom.metrics?.find((m: any) => m.name === 'val_accuracy')
    const valLossMetric = atom.metrics?.find((m: any) => m.name === 'val_loss')
    
    return {
      atom_id: atom.atom_id,
      atom_type: atom.atom_type,
      lifecycle: atom.lifecycle,
      conditions: atom.conditions,
      metrics: atom.metrics,
      val_accuracy: valAccuracyMetric?.value,
      val_loss: valLossMetric?.value,
      optimizer: atom.conditions?.optimizer,
      scheduler: atom.conditions?.scheduler,
      learning_rate: atom.conditions?.learning_rate,
      time_index: index,
    }
  })
}

export function calculateStats(atoms: Atom[]): DashboardStats {
  const totalAtoms = atoms.length
  const trainingRuns = atoms.filter(atom => 
    atom.atom_type === 'finding' && 
    atom.metrics?.some((m: any) => m.name === 'val_accuracy')
  ).length
  const contradictions = atoms.filter(atom => atom.atom_type === 'negative_result').length
  const bounties = atoms.filter(atom => atom.atom_type === 'bounty').length
  const hypotheses = atoms.filter(atom => atom.atom_type === 'hypothesis').length

  return {
    totalAtoms,
    trainingRuns,
    contradictions,
    bounties,
    hypotheses,
  }
}

export function getTopRuns(processedAtoms: ProcessedAtom[]): TopRun[] {
  const runsWithAccuracy = processedAtoms.filter(atom => 
    atom.val_accuracy !== undefined && atom.atom_type === 'finding'
  )

  return runsWithAccuracy
    .sort((a, b) => (b.val_accuracy || 0) - (a.val_accuracy || 0))
    .slice(0, 10)
    .map((atom, index) => ({
      rank: index + 1,
      val_accuracy: atom.val_accuracy || 0,
      val_loss: atom.val_loss,
      optimizer: atom.optimizer || 'unknown',
      scheduler: atom.scheduler || 'unknown',
      learning_rate: atom.learning_rate,
      lifecycle: atom.lifecycle,
      atom_id: atom.atom_id,
    }))
}

export function calculateBestAccuracyOverTime(processedAtoms: ProcessedAtom[]): BestAccuracyPoint[] {
  const runsWithAccuracy = processedAtoms.filter(atom => 
    atom.val_accuracy !== undefined
  ).sort((a, b) => a.time_index - b.time_index)

  const bestAccuracyPoints: BestAccuracyPoint[] = []
  let currentBest = 0

  runsWithAccuracy.forEach(atom => {
    if (atom.val_accuracy! > currentBest) {
      currentBest = atom.val_accuracy!
    }
    bestAccuracyPoints.push({
      time_index: atom.time_index,
      best_accuracy: currentBest,
    })
  })

  return bestAccuracyPoints
}
