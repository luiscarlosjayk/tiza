# Supervised mode

Use this page when the presenter turns on supervised mode and your calls wait. How the presenter reviews your changes is in [Supervise an agent](../guides/supervise-an-agent.md).

Tiza is a terminal tool that draws annotations on code. To connect your client, see [Connect an agent](setup.md).

## For the agent

- Read `supervised` and `pending` in `status`. `supervised` is absent when supervised mode is off, and `pending` is absent when no change waits.
- Each call that changes the document or the screen waits for the decision of the presenter. The list is in [Supervised mode in the Socket API](../reference/socket-api.md#supervised-mode).
- Reads never wait for the presenter: `status`, `get_code`, `render_text`, `check` and the resources. A `check` can wait for another `check`.
- Some requests fail at once and do not wait, for example an unknown method. The list is in [Supervised mode in the Socket API](../reference/socket-api.md#supervised-mode). Other bad parameters fail only when the presenter accepts the change.
- An accepted call gives its usual result. For an annotation, `visible` and `shown_step` are as at the time of the acceptance.
- A rejected call fails with the code `rejected`. The message tells if the presenter rejected the change or if 60 s passed. See [Error codes](../reference/socket-api.md#error-codes).
- Do not send a rejected change again in a loop. Tell the presenter what you want to draw. Wait for the answer.
