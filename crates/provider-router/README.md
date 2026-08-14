# Patchouli Provider Router

Routes every provider primitive by canonical `scope_json`. Ordered rules use
partial exact matches over configured scope fields; the first match wins and an
explicit default handles the remainder. The daemon configuration requires the
only local database to be named `local`. Work units, idempotency, causal/session
frontiers and change cursors are checked to remain in one provider domain.

There is no failure fallback and no cross-provider atomic transaction.
