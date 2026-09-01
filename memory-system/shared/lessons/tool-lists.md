# The tool list is the safety property

An agent that cannot call a tool cannot be talked into calling it. That is a stronger promise
than telling it not to in a prompt, and it is why capability is drawn in the tool list rather
than in the brief.

Where the line sits changed on 2026 08 29. Agents can send mail now. They still cannot trash,
mark spam or change labels, because that is the half where the damage is permanent. A bad turn
can embarrass JJ. It must not be able to lose him a message he needed.

## The flag that eats your prompt

**`--allowedTools` is variadic.** Anything after it on the command line is read as another tool
name. With the tools last, the message was swallowed and `claude` exited saying no input was
given at all, which reached JJ as "miles did not answer".

Reordering the flags works and is one edit away from breaking again. Send the message on stdin.
A pipe cannot be eaten by a flag.
