# Book II — The Governance Book

> Spécification du layer Governance (Sprints 6–8).
> Ce document est incrémental : chaque sprint complète les chapitres correspondants.

---

## Chapter 1: Governance Principles

Le layer Governance implémente la **politique** (business rules) sans modifier le **mécanisme** (Core).

Principes fondamentaux :

* **Traits atomiques** : chaque capability est un trait standalone (`EpochInterceptor`, `SubRoutineHandler`, etc.). Pas d'héritage, pas de `BasePlugin`.
* **Injection par builder** : `BriocheEngineBuilder` force l'injection des traits obligatoires au moment du `build()`. Un trait manquant = erreur de compilation ou `Err` au runtime.
* **Pas de mutation directe de `Session`** : les plugins ne modifient jamais `session.state`, `session.history` ou `session.active_tools` directement. Ils retournent des `PolicyDecision` ou mutent leur propre état dans `ExtensionStorage`.
* **Ordre total des traits** : l'ordre d'appel dans `transition()` est invariant et matérialisé par le code, non par une configuration dynamique.

---

## Chapter 2: Fundamental Traits

### 2.1 EpochInterceptor

```rust
pub trait EpochInterceptor: Send + Sync {
    fn intercept_epoch(&self, input: &EngineInput, ext: &mut ExtensionStorage)
        -> PluginResult<EpochAction>;
}
```

Évalué **en premier** dans chaque cycle. Si `Block`, le kernel retourne immédiatement `Effect::Error(EpochMismatch)` + `SystemIdle`. Aucun trait subséquent ne peut outrepasser une barrière d'époque.

**Implémentation de référence** : `EpochGuard` (`brioche-governance-default`).

### 2.2 SubRoutineHandler

```rust
pub trait SubRoutineHandler: Send + Sync {
    fn handle_subroutine(&self, parent: &mut Session, child: &mut Session, input: &EngineInput)
        -> PluginResult<Option<Vec<Effect>>>;
}
```

Évalué si `session.state == SubRoutine`. Si `Some(effects)`, le dispatch standard est court-circuité. Évalué **après** `EpochInterceptor` (I-Comp-Epoch-Subroutine).

**Implémentation de référence** : `SubRoutineOrchestrator` (`brioche-governance-default`).

### 2.3 ConsistencyVerifier

```rust
pub trait ConsistencyVerifier: Send + Sync {
    fn verify_consistency(&self, session: &mut Session)
        -> PluginResult<Option<Vec<Effect>>>;
}
```

Évalué **en dernier** dans `finalize_transition`. Si `Some(effects)`, le kernel applique un forçage mécanique (typiquement `OverrideTransition` vers `Idle`). Ignoré si `RebuildRoutes` est présent dans les effets.

**Implémentation de référence** : `StateConsistencyGuard` (`brioche-governance-default`).

### 2.4 DecisionAggregator

```rust
pub trait DecisionAggregator: Send + Sync {
    fn aggregate_decisions(&self, decisions: Vec<PolicyDecision>, ext: &mut ExtensionStorage)
        -> PluginResult<PolicyDecision>;
}
```

**Obligatoire.** Agrège les décisions collectées sur `before_prediction`. Sans implémentation injectée, `BriocheEngineBuilder::build()` retourne `Err`.

**Implémentation de référence** : `LexicographicDecisionAggregator` (`brioche-governance-default`).

Règle de fusion :
1. `Block` → court-circuite immédiatement.
2. `OverrideTransition` → premier rencontré l'emporte.
3. `MutateHistory` → accumulation dans l'ordre d'évaluation.
4. `RequestEffect` → premier retourné.
5. `Allow` → ignoré.

### 2.5 SignalDrainOrder

```rust
pub trait SignalDrainOrder: Send + Sync {
    fn drain(&self) -> Vec<EngineInput>;
}
```

**Obligatoire.** Définit l'ordre invariant de drainage des canaux séparés : `SystemSignal` → `GovernanceNotification` → `AsyncTaskResult`. Implémenté par le shell (`SignalMultiplexer`), pas par le kernel.


### 2.6 HookEffectConstraint

```rust
pub trait HookEffectConstraint: Send + Sync {
    fn is_allowed_fast(&self, hook_index: u8, effect_mask: u64) -> bool;
    fn is_allowed_fallback(&self, hook_name: &str, effect_variant: &str) -> bool;
}
```

**Optionnel.** Validation O(1) par masque binaire. Sans injection, tous les `RequestEffect` sont autorisés sur tous les hooks.

**Implémentation de référence** : `FastHookEffectConstraint` (`brioche-governance-default`).

### 2.7 CycleRollbackPolicy

```rust
pub trait CycleRollbackPolicy: Send + Sync {
    fn begin_hook(&self);
    fn on_mutation(&self, type_id: TypeId, vtable: &ExtVTable, current: &dyn Any);
    fn commit_hook(&self);
    fn rollback_hook(&self, ext: &mut ExtensionStorage);
}
```

**Optionnel.** Fournit un mécanisme COW (Copy-On-Write) granular pour restaurer l'état d'`ExtensionStorage` en cas de dépassement de budget. Sans injection, le kernel émet `PluginFault` sans restauration.

> **Note Sprint 6** : l'intégration mécanique dans `ExtensionStorage` (appel automatique de `on_mutation` au premier `get_mut`) est planifiée pour le Sprint 7. Le trait et son implémentation nulle `NoopCycleRollbackPolicy` sont livrés.

**Implémentation de référence** : `NoopCycleRollbackPolicy` (`brioche-governance-default`).

### 2.8 SubRoutineLifecycleGuard

```rust
pub trait SubRoutineLifecycleGuard: Send + Sync {
    fn on_exit(&self, handle: SubRoutineHandle, parent: &mut Session, registry: &mut SessionRegistry)
        -> PluginResult<Vec<Effect>>;
}
```

**Obligatoire.** Appelé par le kernel à chaque transition sortante de `SubRoutine`. Sans implémentation, `BriocheEngineBuilder::build()` retourne `Err`.

**Implémentation de référence** : `SubRoutineCleanupGuard` (`brioche-governance-default`).

### 2.9 GovernanceFailoverHandler

```rust
pub trait GovernanceFailoverHandler: Send + Sync {
    fn handle_failure(&self, session: &mut Session, fault: &Effect)
        -> PluginResult<Option<Vec<Effect>>>;
}
```

**Optionnel.** Filet de sécurité en cas de défaillance en cascade d'un plugin de gouvernance.

**Implémentation de référence** : `SystemFailoverGuard` (`brioche-governance-default`).

### 2.10 CowBudgetPolicy

```rust
pub trait CowBudgetPolicy: Send + Sync {
    fn max_cow_bytes(&self, hook_name: &str) -> usize;
}
```

**Optionnel.** Budget mémoire par hook pour le snapshot COW. Sans injection, la valeur par défaut est 64 KB.

---

## Chapter 3: Integration into the Engine

### 3.1 Ordre d'évaluation dans `transition()`

L'ordre est invariant et codé en dur dans `BriocheEngine::transition()` :

1. **Inject `SessionSnapshot`** dans `ExtensionStorage`.
2. **`EpochInterceptor`** — si `Block`, retour immédiat.
3. **`SubRoutineHandler`** — si `SubRoutine` et `Some(effects)`, short-circuit.
4. **`on_input` hook** — route pré-calculée.
5. **Dispatch principal** (`UserMessage`, `LlmStream`, `ToolCallsResult`, `RestoreSubRoutine`).
6. **`SubRoutineLifecycleGuard`** — si sortie de `SubRoutine`.
7. **`HookEffectConstraint`** — validation des effets émis.
8. **`RebuildRoutes` position guarantee** — tronque tout ce qui suit.
9. **`ConsistencyVerifier`** — sauf si `RebuildRoutes` présent.
10. **`GovernanceFailoverHandler`** — remplace les `PluginFault` si injecté.

### 3.2 Builder mandatory traits

`BriocheEngineBuilder::build()` retourne `Err` si :
- `DecisionAggregator` manquant
- `SubRoutineLifecycleGuard` manquant

Tous les autres traits sont optionnels.

---

## Chapter 4: Default Implementations

Le crate `brioche-governance-default` fournit les implémentations de référence :

| Trait | Implémentation | Fichier |
|-------|----------------|---------|
| `EpochInterceptor` | `EpochGuard` | `epoch_guard.rs` |
| `DecisionAggregator` | `LexicographicDecisionAggregator` | `policy_aggregator.rs` |
| `SubRoutineLifecycleGuard` | `SubRoutineCleanupGuard` | `subroutine_cleanup_guard.rs` |
| `ConsistencyVerifier` | `StateConsistencyGuard` | `state_consistency_guard.rs` |
| `HookEffectConstraint` | `FastHookEffectConstraint` | `hook_effect_constraint.rs` |
| `CycleRollbackPolicy` | `NoopCycleRollbackPolicy` | `noop_rollback_policy.rs` |
| `GovernanceFailoverHandler` | `SystemFailoverGuard` | `system_failover_guard.rs` |
| `SubRoutineHandler` | `SubRoutineOrchestrator` | `subroutine_orchestrator.rs` |

---

*Last updated: 2026-05-26 — Sprint 6 complete*
