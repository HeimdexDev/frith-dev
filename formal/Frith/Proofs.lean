import Frith.Model

namespace Frith

theorem checkExport_iff (m : Manifest) (c : Context) :
    checkExport m c = true ↔ ExportAllowed m c := by
  simp [checkExport, ExportAllowed, and_assoc]

theorem checkExport_sound (h : checkExport m c = true) : ExportAllowed m c :=
  (checkExport_iff m c).mp h

theorem checkExport_complete (h : ExportAllowed m c) : checkExport m c = true :=
  (checkExport_iff m c).mpr h

theorem initial_invariant (m : Manifest) : Invariant (initial m) := by
  simp [Invariant, initial]

theorem step_preserves_invariant (h : step s cmd c = some t) : Invariant t := by
  unfold step at h
  split at h <;> simp_all
  cases ha : cmd.action <;> simp_all [Invariant, advance] <;> grind

theorem reachable_invariant (h : Reachable m s) : Invariant s := by
  induction h with
  | init => exact initial_invariant m
  | next _ hs _ => exact step_preserves_invariant hs

theorem revoked_terminal (h : s.stage = .revoked) : step s cmd c = none := by
  simp [step, h]

theorem stale_revision_denied (h : cmd.expectedRevision ≠ s.revision) :
    step s cmd c = none := by simp [step, h]

theorem manifest_preserved (h : step s cmd c = some t) : t.manifest = s.manifest := by
  unfold step at h
  split at h <;> simp_all
  cases ha : cmd.action <;> simp_all [advance] <;> grind

theorem revision_increases (h : step s cmd c = some t) : t.revision = s.revision + 1 := by
  unfold step at h
  split at h <;> simp_all
  cases ha : cmd.action <;> simp_all [advance] <;> grind

theorem authorization_requires_policy
    (ha : cmd.action = .authorize) (h : step s cmd c = some t) :
    ExportAllowed s.manifest c := by
  unfold step at h
  simp only [ha] at h
  split at h <;> simp_all [checkExport_iff]

theorem publication_requires_policy
    (ha : cmd.action = .publish) (h : step s cmd c = some t) :
    ExportAllowed s.manifest c := by
  unfold step at h
  simp only [ha] at h
  split at h <;> simp_all [checkExport_iff]

end Frith

#print axioms Frith.checkExport_sound
#print axioms Frith.checkExport_complete
#print axioms Frith.reachable_invariant
#print axioms Frith.revoked_terminal
#print axioms Frith.stale_revision_denied
#print axioms Frith.manifest_preserved
#print axioms Frith.revision_increases
#print axioms Frith.authorization_requires_policy
#print axioms Frith.publication_requires_policy
