- `ferrobridge-validator`: a dependency named by a patch range (`4.2.x`) is
  no longer satisfied by a pre-release (`4.2.5-comment`) unless the range
  names that label (`4.2.x-comment`), and the patch must be numeric. The
  rule is the one `scripts/vendor/profile-packages.sh` pins a range by.
