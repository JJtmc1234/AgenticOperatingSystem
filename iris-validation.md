# Iris validation branch

JJ requested one harmless deliberate flaw per local project to exercise Iris.
This branch contains a reporting defect solely for that validation. It is not a
production change and must not be merged. The default branch is unchanged.
Iris must inspect the source, identify the defect and publish a clearly named
test issue before the flaw is repaired here.

## Repair verification

The deliberately planted flaws have been restored to their original source.
The unchanged regression probes failed before repair and passed afterwards.
The probes executed the affected source or extracted date format in isolated local fixtures.
The full validation index and retained probes are under ~/Projects/AOS/iris-validation-20260917.
