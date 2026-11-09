- `ferrobridge-hl7v2`'s map writes an XTN-3 of `CP` as `ContactPoint.system`
  `phone` with `use` `mobile` (when XTN-2 is empty). Before, it wrote
  `system` `mobile`, which the required R4 binding of `ContactPoint.system`
  does not allow. The fix ships as supplements over the guide's
  `table-hl70202-to-contact-point-system` and `datatype-xtn-to-contactpoint`.
