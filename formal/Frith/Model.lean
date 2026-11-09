import Lean

namespace Frith
open Lean

structure Artifact where
  bucket : String
  key : String
  versionId : String
  sha256 : String
  deriving BEq, DecidableEq, Repr, FromJson, ToJson

structure Destination where
  account : String
  region : String
  bucket : String
  key : String
  deriving BEq, DecidableEq, Repr, FromJson, ToJson

structure Manifest where
  schemaVersion : Nat
  tenant : String
  releaseId : String
  generation : Nat
  artifact : Artifact
  destination : Destination
  recipient : String
  purpose : String
  policyVersion : Nat
  modelDigest : String
  basisId : String
  expiresAt : Nat
  deriving BEq, DecidableEq, Repr, FromJson, ToJson

/-- Policy and evidence are supplied by a trusted adapter, never by a recipient. -/
structure Policy where
  schemaVersion : Nat
  tenant : String
  version : Nat
  destination : Destination
  recipient : String
  purpose : String
  modelDigest : String
  basisId : String
  expiresAt : Nat
  approvers : List String
  deriving Repr, FromJson, ToJson

/-- Signature verification and the identity-to-person mapping are outside this model. -/
structure Approval where
  principal : String
  manifest : Manifest
  expiresAt : Nat
  deriving Repr, FromJson, ToJson

structure Review where
  artifact : Artifact
  modelDigest : String
  passed : Bool
  deriving Repr, FromJson, ToJson

structure Context where
  policy : Policy
  first : Approval
  second : Approval
  review : Review
  now : Nat
  deriving Repr, FromJson, ToJson

inductive Stage where
  | pending | authorized | copied | published | revoked
  deriving BEq, DecidableEq, Repr, FromJson, ToJson

structure Receipt where
  destination : Destination
  sha256 : String
  versionId : String
  deriving BEq, DecidableEq, Repr, FromJson, ToJson

structure State where
  manifest : Manifest
  stage : Stage
  revision : Nat
  receipt : Option Receipt
  deriving BEq, DecidableEq, Repr, FromJson, ToJson

inductive Action where
  | authorize | complete | publish | revoke
  deriving BEq, DecidableEq, Repr, FromJson, ToJson

structure Command where
  action : Action
  expectedRevision : Nat
  receipt : Option Receipt
  deriving Repr, FromJson, ToJson

structure Input where
  protocolVersion : Nat
  state : State
  command : Command
  context : Context
  deriving Repr, FromJson, ToJson

/-- Requirements independent of the executable Boolean conjunction below. -/
def ExportAllowed (m : Manifest) (c : Context) : Prop :=
  m.schemaVersion = 1 ∧ c.policy.schemaVersion = 1 ∧
  m.tenant = c.policy.tenant ∧ m.policyVersion = c.policy.version ∧
  m.destination = c.policy.destination ∧ m.recipient = c.policy.recipient ∧
  m.purpose = c.policy.purpose ∧ m.modelDigest = c.policy.modelDigest ∧
  m.basisId = c.policy.basisId ∧
  c.now < m.expiresAt ∧ c.now < c.policy.expiresAt ∧
  c.first.manifest = m ∧ c.second.manifest = m ∧
  c.first.principal ≠ c.second.principal ∧
  c.first.principal ≠ "" ∧ c.second.principal ≠ "" ∧
  c.first.principal ∈ c.policy.approvers ∧ c.second.principal ∈ c.policy.approvers ∧
  c.now < c.first.expiresAt ∧ c.now < c.second.expiresAt ∧
  c.review.artifact = m.artifact ∧ c.review.modelDigest = m.modelDigest ∧
  c.review.passed = true

def checkExport (m : Manifest) (c : Context) : Bool :=
  decide (m.schemaVersion = 1) && decide (c.policy.schemaVersion = 1) &&
  decide (m.tenant = c.policy.tenant) && decide (m.policyVersion = c.policy.version) &&
  decide (m.destination = c.policy.destination) && decide (m.recipient = c.policy.recipient) &&
  decide (m.purpose = c.policy.purpose) && decide (m.modelDigest = c.policy.modelDigest) &&
  decide (m.basisId = c.policy.basisId) &&
  decide (c.now < m.expiresAt) && decide (c.now < c.policy.expiresAt) &&
  decide (c.first.manifest = m) && decide (c.second.manifest = m) &&
  decide (c.first.principal ≠ c.second.principal) &&
  decide (c.first.principal ≠ "") && decide (c.second.principal ≠ "") &&
  decide (c.first.principal ∈ c.policy.approvers) &&
  decide (c.second.principal ∈ c.policy.approvers) &&
  decide (c.now < c.first.expiresAt) && decide (c.now < c.second.expiresAt) &&
  decide (c.review.artifact = m.artifact) && decide (c.review.modelDigest = m.modelDigest) &&
  c.review.passed

def receiptMatches (m : Manifest) (receipt : Option Receipt) : Bool :=
  match receipt with
  | none => false
  | some r => decide (r.destination = m.destination) && decide (r.sha256 = m.artifact.sha256) &&
      decide (r.versionId ≠ "")

/-- Safety concerns verified bytes, not current access. Every new access needs authorization. -/
def Invariant (s : State) : Prop :=
  (s.stage = .copied ∨ s.stage = .published) → receiptMatches s.manifest s.receipt = true

def initial (m : Manifest) : State := ⟨m, .pending, 0, none⟩

def advance (s : State) (stage : Stage) (receipt : Option Receipt) : State :=
  { s with stage := stage, revision := s.revision + 1, receipt := receipt }

/-- Revoke deliberately does not depend on policy-service availability or approval freshness. -/
def step (s : State) (cmd : Command) (c : Context) : Option State :=
  if cmd.expectedRevision ≠ s.revision then none
  else if s.stage = .revoked then none
  else match cmd.action with
  | .authorize =>
      if s.stage = .pending ∧ checkExport s.manifest c = true then
        some (advance s .authorized none) else none
  | .complete =>
      if s.stage = .authorized ∧ receiptMatches s.manifest cmd.receipt = true then
        some (advance s .copied cmd.receipt) else none
  | .publish =>
      if s.stage = .copied ∧ checkExport s.manifest c = true ∧
          receiptMatches s.manifest s.receipt = true then
        some (advance s .published s.receipt) else none
  | .revoke => some (advance s .revoked s.receipt)

inductive Reachable (m : Manifest) : State → Prop where
  | init : Reachable m (initial m)
  | next : Reachable m s → step s cmd c = some t → Reachable m t

end Frith
