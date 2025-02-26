check-features:
  cd idevice
  cargo hack check --feature-powerset --no-dev-deps
  cd ..
