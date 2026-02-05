-- Add pull request tracking to evaluations
-- This enables PR-based CI workflows for GitHub/GitLab/Gitea

-- Add PR-specific columns to evaluations table
ALTER TABLE evaluations ADD COLUMN pr_number INTEGER;
ALTER TABLE evaluations ADD COLUMN pr_head_branch TEXT;
ALTER TABLE evaluations ADD COLUMN pr_base_branch TEXT;
ALTER TABLE evaluations ADD COLUMN pr_action TEXT;

-- Index for efficient PR queries
CREATE INDEX idx_evaluations_pr ON evaluations(jobset_id, pr_number)
    WHERE pr_number IS NOT NULL;
