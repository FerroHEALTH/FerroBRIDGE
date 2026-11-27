// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
// A `getrandom` that returns the same bytes on every call, preloaded into the
// example `hot_path_instructions` by scripts/perf/instructions.sh (#2305).
//
// The standard library seeds every `RandomState` through the weak symbol
// `getrandom`, which it documents as open to interposition for measurements
// that need no randomness. With a random seed, the hash probes of every
// `serde_json` object moved the count by hundreds of instructions between
// runs of one binary. No specification governs this: our own design.
#include <stddef.h>
#include <sys/types.h>

ssize_t getrandom(void *buffer, size_t length, unsigned int flags) {
  (void)flags;
  unsigned char *bytes = buffer;
  for (size_t i = 0; i < length; i++) {
    bytes[i] = (unsigned char)i;
  }
  return (ssize_t)length;
}
