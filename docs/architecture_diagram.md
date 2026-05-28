# mozkeys: System Architecture & Workflow

`mozkeys` is a low-latency, keyboard-driven mouse control utility for Windows. To achieve low latency (<5ms) and deterministic execution, it relies on a multi-threaded architecture with lock-free atomic shared memory.

---

## 1. Thread & Component Architecture

The following diagram illustrates how the threads cooperate using shared state (`KeyStateTable` and `StateMachine`) and Win32 system APIs.

```mermaid
graph TD
    %% Main process entry
    subgraph Main ["Main Thread (main.rs)"]
        InitTimer["1. Init QPC Timer"] --> LoadConfig["2. Load config.toml"]
        LoadConfig --> AllocShared["3. Alloc Shared State (Arc)"]
        AllocShared --> SpawnThreads["4. Spawn Worker Threads"]
    end

    %% Shared state
    subgraph Shared ["Shared Thread-Safe State"]
        KST["KeyStateTable (key_state.rs)<br/>- Flat atomic arrays (256 keys)<br/>- Down/Up timestamps (us)"]
        SM["StateMachine (state_machine.rs)<br/>- Atomic 'active' flag<br/>- Suppression VK lists"]
    end
    AllocShared -.-> KST
    AllocShared -.-> SM

    %% Hook Thread
    subgraph HookThread ["Hook Thread (hook.rs)"]
        HookReg["SetWindowsHookExW (WH_KEYBOARD_LL)"] --> MsgLoop1["Win32 GetMessageW Loop"]
        MsgLoop1 --> Callback["hook_proc(key_event)"]
        Callback --> WriteState["Write transition to KeyStateTable"]
        Callback --> CallSM["StateMachine::on_key_down / on_key_up"]
        Callback --> DecSuppress{"Suppress key?"}
        DecSuppress -- Yes --> RetOne["Return 1 (Eat key)"]
        DecSuppress -- No --> CallNext["CallNextHookEx (Pass key)"]
    end
    SpawnThreads --> HookReg
    WriteState -.-> KST
    CallSM -.-> SM

    %% Movement Thread
    subgraph MovementThread ["Movement Thread (movement_loop.rs)"]
        TimerLoop["240 Hz Loop (Precise QPC Sleep & Spin)"] --> DispatcherTick["Dispatcher::tick()"]
        DispatcherTick --> ReadSM{"Is Mouse Mode Active?"}
        ReadSM -- Yes --> ReadInputs["Read KeyStateTable (Move/Scroll/Click keys)"]
        ReadInputs --> CalcSpeed["Compute velocity (Power curve acceleration)"]
        CalcSpeed --> Normalise["Normalise diagonal direction vectors"]
        Normalise --> SendInput["Win32 SendInput (Mouse move/scroll/clicks)"]
        ReadSM -- No --> ResetState["Reset sub-pixel accumulators & click states"]
    end
    SpawnThreads --> TimerLoop
    ReadSM -.-> SM
    ReadInputs -.-> KST
    SendInput --> OS["Windows Input Queue"]

    %% Overlay Thread
    subgraph OverlayThread ["Overlay Thread (overlay.rs)"]
        CreateWin["Create WS_EX_LAYERED Window"] --> RenderDot["Generate pre-multiplied BGRA lime-green circle"]
        RenderDot --> MsgLoop2["60 Hz WM_TIMER Loop"]
        MsgLoop2 --> PollActive{"Is Mouse Mode Active?"}
        PollActive -- Yes --> ShowOverlay["GetCursorPos() & position window via UpdateLayeredWindow"]
        PollActive -- No --> HideOverlay["Hide overlay window via ShowWindow"]
    end
    SpawnThreads --> CreateWin
    PollActive -.-> SM
```

---

## 2. Dynamic Input Event Sequence

The sequence diagram below visualizes:
1. **Scenario A**: Toggle sequence activating the mouse mode (CapsLock double-tap default configuration).
2. **Scenario B**: The periodic thread dispatch computing acceleration and sending cursor movements to the operating system.

```mermaid
sequenceDiagram
    autonumber
    actor User as User Keyboard Input
    participant Hook as Hook Thread (hook.rs)
    participant KST as KeyStateTable (key_state.rs)
    participant SM as StateMachine (state_machine.rs)
    participant Mov as Movement Loop (movement_loop.rs)
    participant Disp as Dispatcher (dispatcher.rs)
    participant OS as Win32 OS / Focus App

    %% Trigger Double Tap
    Note over User, OS: Scenario A: User double-taps CapsLock to toggle Mouse Mode
    User->>Hook: Press CapsLock (Down)
    Hook->>KST: set_down(VK_CAPITAL)
    Hook->>SM: on_key_down(VK_CAPITAL)
    SM->>SM: trigger_held = true, suppress CapsLock
    Hook-->>User: Suppress Key (Eat input, return LRESULT 1)
    
    User->>Hook: Release CapsLock (Up)
    Hook->>KST: set_up(VK_CAPITAL)
    Hook->>SM: on_key_up(VK_CAPITAL, now_us)
    SM->>SM: last_trigger_up = now_us
    Hook-->>User: Suppress Key (Eat input)

    User->>Hook: Press CapsLock (Down)
    Hook->>KST: set_down(VK_CAPITAL)
    Hook->>SM: on_key_down(VK_CAPITAL)
    Hook-->>User: Suppress Key (Eat input)

    User->>Hook: Release CapsLock (Up)
    Hook->>KST: set_up(VK_CAPITAL)
    Hook->>SM: on_key_up(VK_CAPITAL, now_us)
    SM->>SM: Check delta: (now_us - last_trigger_up) <= 250ms
    SM->>SM: Toggle active status (active = true)
    Hook-->>User: Suppress Key (Eat input)

    %% Tick Loop Movement
    Note over User, OS: Scenario B: Mouse Mode active, movement tick occurs
    loop Every 4.16ms (240 Hz)
        Mov->>Disp: tick(now_us)
        Disp->>SM: is_active()
        SM-->>Disp: true
        Disp->>KST: Check key states (Up/Down/Left/Right/Precision)
        KST-->>Disp: Up held (dur = 10ms), Precision held (true)
        Disp->>Disp: Calculate velocity curve & apply precision multiplier
        Disp->>OS: SendInput(MOUSEEVENTF_MOVE, 0, -dy)
    end
```

---

## 3. Core Architectural Highlights

- **Lock-Free Concurrency**: Synchronisation between the critical **Hook Thread** (which must return in $< 1\,\text{ms}$) and the **Movement Thread** uses atomic fields (`AtomicBool`, `AtomicU64`) inside `KeyStateTable` and `StateMachine`. There are no mutexes or allocations in the critical hot path.
- **Power Curve Acceleration**: Movement ticks compute velocity dynamically:
  $$v(t) = \min(\text{base\_speed} + \text{acceleration} \times t^{1.5}, \text{max\_speed})$$
  This curve scales based on how long ($t$) the key has been held down.
- **Sub-Pixel Precision**: Floating-point displacements are accumulated inside `Dispatcher::accum_x` and `Dispatcher::accum_y`. Only when the accumulated displacement exceeds $\pm 1$ pixel is a Win32 `SendInput` event dispatched, ensuring smooth movement at ultra-low speeds.
- **DPI-Aware Layered Overlay**: The indicator overlay is drawn onto a custom 32-bit pre-multiplied BGRA memory DC bitmap, rendering a smooth anti-aliased lime green dot that updates dynamically at 60 Hz without stealing keyboard focus or capturing click events.
