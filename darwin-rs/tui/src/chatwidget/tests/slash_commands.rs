use super::*;
use pretty_assertions::assert_eq;

fn turn_complete_event(turn_id: &str, last_agent_message: Option<&str>) -> TurnCompleteEvent {
    serde_json::from_value(serde_json::json!({
        "turn_id": turn_id,
        "last_agent_message": last_agent_message,
    }))
    .expect("turn complete event should deserialize")
}

fn submit_composer_text(chat: &mut ChatWidget, text: &str) {
    chat.bottom_pane
        .set_composer_text(text.to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

fn recall_latest_after_clearing(chat: &mut ChatWidget) -> String {
    chat.bottom_pane
        .set_composer_text(String::new(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    chat.bottom_pane.composer_text()
}

fn drain_app_events(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) -> Vec<AppEvent> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>()
}

#[tokio::test]
async fn slash_compact_eagerly_queues_follow_up_before_turn_start() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Compact);

    assert!(chat.bottom_pane.is_task_running());
    match rx.try_recv() {
        Ok(AppEvent::DarwinCodeOp(Op::Compact)) => {}
        other => panic!("expected compact op to be submitted, got {other:?}"),
    }

    chat.bottom_pane.set_composer_text(
        "queued before compact turn start".to_string(),
        Vec::new(),
        Vec::new(),
    );
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(chat.pending_steers.is_empty());
    assert_eq!(chat.queued_user_messages.len(), 1);
    assert_eq!(
        chat.queued_user_messages.front().unwrap().text,
        "queued before compact turn start"
    );
    assert_matches!(op_rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn ctrl_d_quits_without_prompt() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_matches!(rx.try_recv(), Ok(AppEvent::Exit(ExitMode::ShutdownFirst)));
}

#[tokio::test]
async fn ctrl_d_with_modal_open_does_not_quit() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.open_approvals_popup();
    chat.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));

    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn slash_init_skips_when_project_doc_exists() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let tempdir = tempdir().unwrap();
    let existing_path = tempdir.path().join(DEFAULT_AGENTS_MD_FILENAME);
    std::fs::write(&existing_path, "existing instructions").unwrap();
    chat.config.cwd = tempdir.path().to_path_buf().abs();

    submit_composer_text(&mut chat, "/init");

    match op_rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        other => panic!("expected no DarwinCode op to be sent, got {other:?}"),
    }

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains(DEFAULT_AGENTS_MD_FILENAME),
        "info message should mention the existing file: {rendered:?}"
    );
    assert!(
        rendered.contains("Skipping /init"),
        "info message should explain why /init was skipped: {rendered:?}"
    );
    assert_eq!(
        std::fs::read_to_string(existing_path).unwrap(),
        "existing instructions"
    );
    assert_eq!(recall_latest_after_clearing(&mut chat), "/init");
}

#[tokio::test]
async fn bare_slash_command_is_available_from_local_recall_after_dispatch() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/diff");

    let _ = drain_insert_history(&mut rx);
    chat.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(chat.bottom_pane.composer_text(), "/diff");
}

#[tokio::test]
async fn inline_slash_command_is_available_from_local_recall_after_dispatch() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/rename Better title");

    let _ = drain_insert_history(&mut rx);
    chat.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(chat.bottom_pane.composer_text(), "/rename Better title");
}

#[tokio::test]
async fn slash_rename_prefills_existing_thread_name() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_name = Some("Current project title".to_string());

    chat.dispatch_command(SlashCommand::Rename);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("slash_rename_prefilled_prompt", popup);

    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::DarwinCodeOp(Op::SetThreadName { name })) if name == "Current project title"
    );
}

#[tokio::test]
async fn slash_rename_without_existing_thread_name_starts_empty() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Rename);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(popup.contains("Name thread"));
    assert!(popup.contains("Type a name and press Enter"));

    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn usage_error_slash_command_is_available_from_local_recall() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.3-darwin_code")).await;
    chat.set_feature_enabled(Feature::FastMode, /*enabled*/ true);

    submit_composer_text(&mut chat, "/fast maybe");

    assert_eq!(chat.bottom_pane.composer_text(), "");

    let cells = drain_insert_history(&mut rx);
    let rendered = cells
        .iter()
        .map(|cell| lines_to_single_string(cell))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("Usage: /fast [on|off|status]"),
        "expected usage message, got: {rendered:?}"
    );
    assert_eq!(recall_latest_after_clearing(&mut chat), "/fast maybe");
}

#[tokio::test]
async fn unrecognized_slash_command_is_not_added_to_local_recall() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/does-not-exist");

    let cells = drain_insert_history(&mut rx);
    let rendered = cells
        .iter()
        .map(|cell| lines_to_single_string(cell))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("Unrecognized command '/does-not-exist'"),
        "expected unrecognized-command message, got: {rendered:?}"
    );
    assert_eq!(chat.bottom_pane.composer_text(), "/does-not-exist");
    assert_eq!(recall_latest_after_clearing(&mut chat), "");
}

#[tokio::test]
async fn unavailable_slash_command_is_available_from_local_recall() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.bottom_pane.set_task_running(/*running*/ true);

    submit_composer_text(&mut chat, "/model");

    let cells = drain_insert_history(&mut rx);
    let rendered = cells
        .iter()
        .map(|cell| lines_to_single_string(cell))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("'/model' is disabled while a task is in progress."),
        "expected disabled-command message, got: {rendered:?}"
    );
    assert_eq!(recall_latest_after_clearing(&mut chat), "/model");
}

#[tokio::test]
async fn no_op_stub_slash_command_is_available_from_local_recall() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/debug-m-drop");

    let cells = drain_insert_history(&mut rx);
    let rendered = cells
        .iter()
        .map(|cell| lines_to_single_string(cell))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("Memory maintenance"),
        "expected stub message, got: {rendered:?}"
    );
    assert_eq!(recall_latest_after_clearing(&mut chat), "/debug-m-drop");
}

#[tokio::test]
async fn slash_quit_requests_exit() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Quit);

    assert_matches!(rx.try_recv(), Ok(AppEvent::Exit(ExitMode::ShutdownFirst)));
}

#[tokio::test]
async fn slash_copy_state_tracks_turn_complete_final_reply() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_darwin_code_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: Some("Final reply **markdown**".to_string()),
            completed_at: None,
            duration_ms: None,
        }),
    });

    assert_eq!(
        chat.last_agent_markdown_text(),
        Some("Final reply **markdown**")
    );
}

#[tokio::test]
async fn slash_copy_state_tracks_plan_item_completion() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let plan_text = "## Plan\n\n1. Build it\n2. Test it".to_string();

    chat.handle_darwin_code_event(Event {
        id: "item-plan".into(),
        msg: EventMsg::ItemCompleted(ItemCompletedEvent {
            thread_id: ThreadId::new(),
            turn_id: "turn-1".to_string(),
            item: TurnItem::Plan(PlanItem {
                id: "plan-1".to_string(),
                text: plan_text.clone(),
            }),
        }),
    });
    chat.handle_darwin_code_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
            completed_at: None,
            duration_ms: None,
        }),
    });

    assert_eq!(chat.last_agent_markdown_text(), Some(plan_text.as_str()));
    assert_matches!(
        chat.pending_notification,
        Some(Notification::AgentTurnComplete { ref response }) if response == &plan_text
    );
}

#[tokio::test]
async fn slash_copy_reports_when_no_agent_response_exists() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Copy);

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert_chatwidget_snapshot!("slash_copy_no_output_info_message", rendered);
    assert!(
        rendered.contains("No agent response to copy"),
        "expected no-output message, got {rendered:?}"
    );
}

#[tokio::test]
async fn ctrl_o_copy_reports_when_no_agent_response_exists() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_key_event(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("No agent response to copy"),
        "expected no-output message, got {rendered:?}"
    );
}

#[tokio::test]
async fn slash_copy_stores_clipboard_lease_and_preserves_it_on_failure() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.last_agent_markdown = Some("copy me".to_string());

    chat.copy_last_agent_markdown_with(|markdown| {
        assert_eq!(markdown, "copy me");
        Ok(Some(crate::clipboard_copy::ClipboardLease::test()))
    });

    assert!(chat.clipboard_lease.is_some());
    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one success message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("Copied last message to clipboard"),
        "expected success message, got {rendered:?}"
    );

    chat.copy_last_agent_markdown_with(|markdown| {
        assert_eq!(markdown, "copy me");
        Err("blocked".into())
    });

    assert!(chat.clipboard_lease.is_some());
    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one failure message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("Copy failed: blocked"),
        "expected failure message, got {rendered:?}"
    );
}

#[tokio::test]
async fn slash_copy_state_is_preserved_during_running_task() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_darwin_code_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: Some("Previous completed reply".to_string()),
            completed_at: None,
            duration_ms: None,
        }),
    });
    chat.on_task_started();

    assert_eq!(
        chat.last_agent_markdown_text(),
        Some("Previous completed reply")
    );
}

#[tokio::test]
async fn slash_copy_tracks_replayed_legacy_agent_message_when_turn_complete_omits_text() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_darwin_code_event_replay(Event {
        id: "turn-1".into(),
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Legacy final message".into(),
            phase: None,
            memory_citation: None,
        }),
    });
    let _ = drain_insert_history(&mut rx);
    chat.handle_darwin_code_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
            completed_at: None,
            duration_ms: None,
        }),
    });
    let _ = drain_insert_history(&mut rx);

    assert_eq!(
        chat.last_agent_markdown_text(),
        Some("Legacy final message")
    );
}

#[tokio::test]
async fn slash_copy_uses_agent_message_item_when_turn_complete_omits_final_text() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_darwin_code_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            started_at: None,
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    complete_assistant_message(
        &mut chat,
        "msg-1",
        "Legacy item final message",
        /*phase*/ None,
    );
    let _ = drain_insert_history(&mut rx);
    chat.handle_darwin_code_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
            completed_at: None,
            duration_ms: None,
        }),
    });
    let _ = drain_insert_history(&mut rx);

    assert_eq!(
        chat.last_agent_markdown_text(),
        Some("Legacy item final message")
    );
    assert_matches!(
        chat.pending_notification,
        Some(Notification::AgentTurnComplete { ref response }) if response == "Legacy item final message"
    );
}

#[tokio::test]
async fn agent_turn_complete_notification_does_not_reuse_stale_copy_source() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_darwin_code_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(turn_complete_event("turn-1", Some("Previous reply"))),
    });
    chat.pending_notification = None;

    chat.handle_darwin_code_event(Event {
        id: "turn-2".into(),
        msg: EventMsg::TurnComplete(turn_complete_event(
            "turn-2", /*last_agent_message*/ None,
        )),
    });

    assert_matches!(
        chat.pending_notification,
        Some(Notification::AgentTurnComplete { ref response }) if response.is_empty()
    );
}

#[tokio::test]
async fn slash_exit_requests_exit() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Exit);

    assert_matches!(rx.try_recv(), Ok(AppEvent::Exit(ExitMode::ShutdownFirst)));
}

#[tokio::test]
async fn slash_stop_submits_background_terminal_cleanup() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Stop);

    assert_matches!(op_rx.try_recv(), Ok(Op::CleanBackgroundTerminals));
    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected cleanup confirmation message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("Stopping all background terminals."),
        "expected cleanup confirmation, got {rendered:?}"
    );
}

#[tokio::test]
async fn slash_clear_requests_ui_clear_when_idle() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Clear);

    assert_matches!(rx.try_recv(), Ok(AppEvent::ClearUi));
}

#[tokio::test]
async fn slash_clear_is_disabled_while_task_running() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.bottom_pane.set_task_running(/*running*/ true);

    chat.dispatch_command(SlashCommand::Clear);

    let event = rx.try_recv().expect("expected disabled command error");
    match event {
        AppEvent::InsertHistoryCell(cell) => {
            let rendered = lines_to_single_string(&cell.display_lines(/*width*/ 80));
            assert!(
                rendered.contains("'/clear' is disabled while a task is in progress."),
                "expected /clear task-running error, got {rendered:?}"
            );
        }
        other => panic!("expected InsertHistoryCell error, got {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "expected no follow-up events");
}

#[tokio::test]
async fn slash_memory_drop_reports_stubbed_feature() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::MemoryDrop);

    let event = rx.try_recv().expect("expected unsupported-feature error");
    match event {
        AppEvent::InsertHistoryCell(cell) => {
            let rendered = lines_to_single_string(&cell.display_lines(/*width*/ 80));
            assert!(rendered.contains("Memory maintenance: Not available in TUI yet."));
        }
        other => panic!("expected InsertHistoryCell error, got {other:?}"),
    }
    assert!(
        op_rx.try_recv().is_err(),
        "expected no memory op to be sent"
    );
}

#[tokio::test]
async fn slash_mcp_requests_inventory_via_app_server() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Mcp);

    assert!(active_blob(&chat).contains("Loading MCP inventory"));
    loop {
        match rx.try_recv() {
            Ok(AppEvent::InsertHistoryCell(cell))
                if cell
                    .as_any()
                    .is::<crate::history_cell::SessionHeaderHistoryCell>() =>
            {
                continue;
            }
            other => {
                assert_matches!(other, Ok(AppEvent::FetchMcpInventory));
                break;
            }
        }
    }
    assert!(op_rx.try_recv().is_err(), "expected no core op to be sent");
}

#[tokio::test]
async fn slash_memories_opens_memory_menu() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::MemoryTool, /*enabled*/ true);

    chat.dispatch_command(SlashCommand::Memories);

    assert!(render_bottom_popup(&chat, /*width*/ 80).contains("Use memories"));
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
    assert!(op_rx.try_recv().is_err(), "expected no core op to be sent");
}

#[tokio::test]
async fn slash_memory_update_reports_stubbed_feature() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::MemoryUpdate);

    let event = rx.try_recv().expect("expected unsupported-feature error");
    match event {
        AppEvent::InsertHistoryCell(cell) => {
            let rendered = lines_to_single_string(&cell.display_lines(/*width*/ 80));
            assert!(rendered.contains("Memory maintenance: Not available in TUI yet."));
        }
        other => panic!("expected InsertHistoryCell error, got {other:?}"),
    }
    assert!(
        op_rx.try_recv().is_err(),
        "expected no memory op to be sent"
    );
}

#[tokio::test]
async fn slash_resume_opens_picker() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Resume);

    assert_matches!(rx.try_recv(), Ok(AppEvent::OpenResumePicker));
}

#[tokio::test]
async fn slash_resume_with_arg_requests_named_session() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.bottom_pane.set_composer_text(
        "/resume my-saved-thread".to_string(),
        Vec::new(),
        Vec::new(),
    );
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::ResumeSessionByIdOrName(id_or_name)) if id_or_name == "my-saved-thread"
    );
    assert_matches!(op_rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn slash_fork_requests_current_fork() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Fork);

    assert_matches!(rx.try_recv(), Ok(AppEvent::ForkCurrentSession));
}

#[tokio::test]
async fn slash_rollout_displays_current_path() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let rollout_path = PathBuf::from("/tmp/darwin_code-test-rollout.jsonl");
    chat.current_rollout_path = Some(rollout_path.clone());

    chat.dispatch_command(SlashCommand::Rollout);

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected info message for rollout path");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains(&rollout_path.display().to_string()),
        "expected rollout path to be shown: {rendered}"
    );
}

#[tokio::test]
async fn slash_rollout_handles_missing_path() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Rollout);

    let cells = drain_insert_history(&mut rx);
    assert_eq!(
        cells.len(),
        1,
        "expected info message explaining missing path"
    );
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("not available"),
        "expected missing rollout path message: {rendered}"
    );
}

#[tokio::test]
async fn undo_success_events_render_info_messages() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_darwin_code_event(Event {
        id: "turn-1".to_string(),
        msg: EventMsg::UndoStarted(UndoStartedEvent {
            message: Some("Undo requested for the last turn...".to_string()),
        }),
    });
    assert!(
        chat.bottom_pane.status_indicator_visible(),
        "status indicator should be visible during undo"
    );

    chat.handle_darwin_code_event(Event {
        id: "turn-1".to_string(),
        msg: EventMsg::UndoCompleted(UndoCompletedEvent {
            success: true,
            message: None,
        }),
    });

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected final status only");
    assert!(
        !chat.bottom_pane.status_indicator_visible(),
        "status indicator should be hidden after successful undo"
    );

    let completed = lines_to_single_string(&cells[0]);
    assert!(
        completed.contains("Undo completed successfully."),
        "expected default success message, got {completed:?}"
    );
}

#[tokio::test]
async fn undo_failure_events_render_error_message() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_darwin_code_event(Event {
        id: "turn-2".to_string(),
        msg: EventMsg::UndoStarted(UndoStartedEvent { message: None }),
    });
    assert!(
        chat.bottom_pane.status_indicator_visible(),
        "status indicator should be visible during undo"
    );

    chat.handle_darwin_code_event(Event {
        id: "turn-2".to_string(),
        msg: EventMsg::UndoCompleted(UndoCompletedEvent {
            success: false,
            message: Some("Failed to restore workspace state.".to_string()),
        }),
    });

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected final status only");
    assert!(
        !chat.bottom_pane.status_indicator_visible(),
        "status indicator should be hidden after failed undo"
    );

    let completed = lines_to_single_string(&cells[0]);
    assert!(
        completed.contains("Failed to restore workspace state."),
        "expected failure message, got {completed:?}"
    );
}

#[tokio::test]
async fn undo_started_hides_interrupt_hint() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_darwin_code_event(Event {
        id: "turn-hint".to_string(),
        msg: EventMsg::UndoStarted(UndoStartedEvent { message: None }),
    });

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be active");
    assert!(
        !status.interrupt_hint_visible(),
        "undo should hide the interrupt hint because the operation cannot be cancelled"
    );
}

#[tokio::test]
async fn model_picker_single_effort_selection_closes_without_reasoning_popup() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.3-darwin_code")).await;
    set_fast_mode_test_catalog(&mut chat);
    let mut preset = get_available_model(&chat, "gpt-5.4");
    preset.model = "single-effort-test".to_string();
    preset.id = preset.model.clone();
    preset.display_name = preset.model.clone();
    let expected_effort = Some(preset.default_reasoning_effort);

    chat.open_all_models_popup(vec![preset.clone()]);
    assert!(!chat.no_modal_or_popup_active());

    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));
    let events = drain_app_events(&mut rx);

    assert!(
        chat.no_modal_or_popup_active(),
        "single-effort model selection should close the picker"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AppEvent::OpenReasoningPopup { .. })),
        "single-effort selection should not open a no-op reasoning popup: {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::UpdateModelSelection { model } if model == "single-effort-test"
        )),
        "expected model update event, got {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::UpdateReasoningEffort(effort) if *effort == expected_effort
        )),
        "expected reasoning effort update event, got {events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                AppEvent::PersistModelSelection { model, effort }
                    if model == "single-effort-test" && *effort == expected_effort
            ))
            .count(),
        1,
        "expected exactly one persisted model selection, got {events:?}"
    );
}

#[tokio::test]
async fn model_picker_default_effort_selection_closes_when_catalog_has_no_choices() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.3-darwin_code")).await;
    set_fast_mode_test_catalog(&mut chat);
    let mut preset = get_available_model(&chat, "gpt-5.4");
    preset.model = "deepseek-v4-flash".to_string();
    preset.id = preset.model.clone();
    preset.provider_id = Some("deepseek".to_string());
    preset.display_name = preset.model.clone();
    preset.supported_reasoning_efforts.clear();
    let expected_effort = Some(preset.default_reasoning_effort);

    chat.open_all_models_popup(vec![preset]);
    assert!(!chat.no_modal_or_popup_active());

    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));
    let events = drain_app_events(&mut rx);

    assert!(
        chat.no_modal_or_popup_active(),
        "default-effort model selection should close the picker when no choices are advertised"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AppEvent::OpenReasoningPopup { .. })),
        "empty-choice selection should not leave a parent picker waiting for a child: {events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                AppEvent::UpdateModelSelection { model } if model == "deepseek-v4-flash"
            ))
            .count(),
        1,
        "expected exactly one model update, got {events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                AppEvent::PersistModelSelection { model, effort }
                    if model == "deepseek-v4-flash" && *effort == expected_effort
            ))
            .count(),
        1,
        "expected exactly one persisted model selection, got {events:?}"
    );

    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));
    let follow_up_events = drain_app_events(&mut rx);
    assert!(
        !follow_up_events
            .iter()
            .any(|event| matches!(event, AppEvent::PersistModelSelection { .. })),
        "second Enter should not re-select a stale model picker: {follow_up_events:?}"
    );
}

#[tokio::test]
async fn provider_model_picker_search_matches_provider_rows() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.3-darwin_code")).await;
    set_fast_mode_test_catalog(&mut chat);
    let template = get_available_model(&chat, "gpt-5.4");

    let mut deepseek_flash = template.clone();
    deepseek_flash.model = "deepseek-v4-flash".to_string();
    deepseek_flash.id = deepseek_flash.model.clone();
    deepseek_flash.display_name = "DeepSeek V4 Flash".to_string();
    deepseek_flash.description = "DeepSeek provider".to_string();
    deepseek_flash.provider_id = Some("deepseek".to_string());

    let mut deepseek_pro = template.clone();
    deepseek_pro.model = "deepseek-v4-pro".to_string();
    deepseek_pro.id = deepseek_pro.model.clone();
    deepseek_pro.display_name = "DeepSeek V4 Pro".to_string();
    deepseek_pro.description = "DeepSeek provider".to_string();
    deepseek_pro.provider_id = Some("deepseek".to_string());

    let mut qwen = template;
    qwen.model = "qwen3.6-plus".to_string();
    qwen.id = qwen.model.clone();
    qwen.display_name = "Qwen 3.6 Plus".to_string();
    qwen.description = "Qwen provider".to_string();
    qwen.provider_id = Some("qwen".to_string());

    chat.open_all_models_popup(vec![qwen, deepseek_flash, deepseek_pro]);
    for ch in "deeps".chars() {
        chat.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }

    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        popup.contains("deepseek-v4-flash"),
        "provider search should show DeepSeek flash row, got:\n{popup}"
    );
    assert!(
        popup.contains("deepseek-v4-pro"),
        "provider search should show DeepSeek pro row, got:\n{popup}"
    );
    assert!(
        !popup.contains("qwen3.6-plus"),
        "provider search should hide non-matching provider rows, got:\n{popup}"
    );
}

#[tokio::test]
async fn model_picker_orders_current_default_then_provider_and_model() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("qwen3.6-plus")).await;
    chat.set_model("qwen3.6-plus");
    chat.set_model_provider("qwen", None);
    set_fast_mode_test_catalog(&mut chat);
    let template = get_available_model(&chat, "gpt-5.4");

    let mut current = template.clone();
    current.model = "qwen3.6-plus".to_string();
    current.id = current.model.clone();
    current.display_name = "Qwen 3.6 Plus".to_string();
    current.description = "Qwen provider".to_string();
    current.provider_id = Some("qwen".to_string());
    current.is_default = false;

    let mut default = template.clone();
    default.model = "deepseek-v4-flash".to_string();
    default.id = default.model.clone();
    default.display_name = "DeepSeek V4 Flash".to_string();
    default.description = "DeepSeek provider".to_string();
    default.provider_id = Some("deepseek".to_string());
    default.is_default = true;

    let mut deepseek_pro = template.clone();
    deepseek_pro.model = "deepseek-v4-pro".to_string();
    deepseek_pro.id = deepseek_pro.model.clone();
    deepseek_pro.display_name = "DeepSeek V4 Pro".to_string();
    deepseek_pro.description = "DeepSeek provider".to_string();
    deepseek_pro.provider_id = Some("deepseek".to_string());
    deepseek_pro.is_default = false;

    let mut claude = template.clone();
    claude.model = "claude-sonnet".to_string();
    claude.id = claude.model.clone();
    claude.display_name = "Claude Sonnet".to_string();
    claude.description = "Claude provider".to_string();
    claude.provider_id = Some("claude".to_string());
    claude.is_default = false;

    let mut qwen_coder = template;
    qwen_coder.model = "qwen-coder".to_string();
    qwen_coder.id = qwen_coder.model.clone();
    qwen_coder.display_name = "Qwen Coder".to_string();
    qwen_coder.description = "Qwen provider".to_string();
    qwen_coder.provider_id = Some("qwen".to_string());
    qwen_coder.is_default = false;

    chat.open_all_models_popup(vec![qwen_coder, deepseek_pro, default, current, claude]);
    let popup = render_bottom_popup(&chat, /*width*/ 120);

    let qwen_current = popup.find("qwen3.6-plus (current)").expect(&popup);
    let deepseek_default = popup.find("deepseek-v4-flash (default)").expect(&popup);
    let claude = popup.find("claude-sonnet").expect(&popup);
    let deepseek_pro = popup.find("deepseek-v4-pro").expect(&popup);
    let qwen_coder = popup.find("qwen-coder").expect(&popup);

    assert!(
        qwen_current < deepseek_default
            && deepseek_default < claude
            && claude < deepseek_pro
            && deepseek_pro < qwen_coder,
        "model picker order should be current, default, then provider/model alphabetic:\n{popup}"
    );
}

#[tokio::test]
async fn model_picker_multi_effort_selection_still_opens_reasoning_popup() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.3-darwin_code")).await;
    set_fast_mode_test_catalog(&mut chat);
    let mut preset = get_available_model(&chat, "gpt-5.4");
    preset.model = "multi-effort-test".to_string();
    preset.id = preset.model.clone();
    preset.display_name = preset.model.clone();
    preset.supported_reasoning_efforts.push(
        darwin_code_protocol::model_metadata::ReasoningEffortPreset {
            effort: ReasoningEffortConfig::High,
            description: "high".to_string(),
        },
    );

    chat.open_all_models_popup(vec![preset]);
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));
    let events = drain_app_events(&mut rx);

    assert!(
        !chat.no_modal_or_popup_active(),
        "multi-effort model selection should keep the parent until the child effort popup accepts"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::OpenReasoningPopup { model } if model.model == "multi-effort-test"
        )),
        "multi-effort selection should still open reasoning popup, got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AppEvent::PersistModelSelection { .. })),
        "multi-effort model should not persist until an effort is selected: {events:?}"
    );
}

#[tokio::test]
async fn model_picker_single_effort_plan_mode_opens_scope_prompt() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.4")).await;
    set_fast_mode_test_catalog(&mut chat);
    let plan_mask =
        collaboration_modes::plan_mask(chat.model_catalog.as_ref()).expect("plan mode preset");
    chat.set_collaboration_mask(plan_mask);
    let mut preset = get_available_model(&chat, "gpt-5.4");
    preset.supported_reasoning_efforts = vec![
        darwin_code_protocol::model_metadata::ReasoningEffortPreset {
            effort: ReasoningEffortConfig::High,
            description: "high".to_string(),
        },
    ];
    preset.default_reasoning_effort = ReasoningEffortConfig::High;

    chat.open_all_models_popup(vec![preset]);
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));
    let events = drain_app_events(&mut rx);

    assert!(
        !chat.no_modal_or_popup_active(),
        "Plan-mode scope prompt should leave the parent available until the child resolves"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::OpenPlanReasoningScopePrompt { model, effort }
                if model == "gpt-5.4" && *effort == Some(ReasoningEffortConfig::High)
        )),
        "single-effort Plan-mode selection should open scope prompt, got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AppEvent::PersistModelSelection { .. })),
        "Plan-mode scope prompt should defer persistence until scope is selected: {events:?}"
    );
}

#[tokio::test]
async fn fast_slash_command_updates_and_persists_local_service_tier() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5.3-darwin_code")).await;
    chat.set_feature_enabled(Feature::FastMode, /*enabled*/ true);

    chat.dispatch_command(SlashCommand::Fast);

    let events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::DarwinCodeOp(Op::OverrideTurnContext {
                service_tier: Some(Some(ServiceTier::Fast)),
                ..
            })
        )),
        "expected fast-mode override app event; events: {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::PersistServiceTierSelection {
                service_tier: Some(ServiceTier::Fast),
            }
        )),
        "expected fast-mode persistence app event; events: {events:?}"
    );

    assert_matches!(op_rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn user_turn_carries_service_tier_after_fast_toggle() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5.3-darwin_code")).await;
    chat.thread_id = Some(ThreadId::new());
    set_byok_model_catalog(&mut chat);
    chat.set_feature_enabled(Feature::FastMode, /*enabled*/ true);

    chat.dispatch_command(SlashCommand::Fast);

    let _events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();

    chat.bottom_pane
        .set_composer_text("hello".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    match next_submit_op(&mut op_rx) {
        Op::UserTurn {
            service_tier: Some(Some(ServiceTier::Fast)),
            ..
        } => {}
        other => panic!("expected Op::UserTurn with fast service tier, got {other:?}"),
    }
}

#[tokio::test]
async fn user_turn_clears_service_tier_after_fast_is_turned_off() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5.3-darwin_code")).await;
    chat.thread_id = Some(ThreadId::new());
    set_byok_model_catalog(&mut chat);
    chat.set_feature_enabled(Feature::FastMode, /*enabled*/ true);

    chat.dispatch_command(SlashCommand::Fast);
    let _events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();

    chat.dispatch_command_with_args(SlashCommand::Fast, "off".to_string(), Vec::new());
    let _events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();

    chat.bottom_pane
        .set_composer_text("hello".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    match next_submit_op(&mut op_rx) {
        Op::UserTurn {
            service_tier: Some(None),
            ..
        } => {}
        other => panic!("expected Op::UserTurn to clear service tier, got {other:?}"),
    }
}

#[tokio::test]
async fn compact_queues_user_messages_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.handle_darwin_code_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            started_at: None,
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    chat.submit_user_message(UserMessage::from(
        "Steer submitted while /compact was running.".to_string(),
    ));
    chat.handle_darwin_code_event(Event {
        id: "steer-rejected".into(),
        msg: EventMsg::Error(ErrorEvent {
            message: "cannot steer a compact turn".to_string(),
            darwin_code_error_info: Some(DarwinCodeErrorInfo::ActiveTurnNotSteerable {
                turn_kind: NonSteerableTurnKind::Compact,
            }),
        }),
    });

    let width: u16 = 80;
    let height: u16 = 18;
    let backend = VT100Backend::new(width, height);
    let mut term = crate::custom_terminal::Terminal::with_options(backend).expect("terminal");
    let desired_height = chat.desired_height(width).min(height);
    term.set_viewport_area(Rect::new(0, height - desired_height, width, desired_height));
    term.draw(|f| {
        chat.render(f.area(), f.buffer_mut());
    })
    .unwrap();
    assert_chatwidget_snapshot!(
        "compact_queues_user_messages_snapshot",
        normalize_snapshot_paths(term.backend().vt100().screen().contents())
    );
}
