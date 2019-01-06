ENV := # pass from command line
AWS_REGION := ap-northeast-1
AWS_CLOUDFORMATION_STACK_NAME := SlackNewChannelNotificationStack
AWS_S3_BUCKET := # pass from command line

# see: https://postd.cc/auto-documented-makefile/
.PHONY: help
help: ## show help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'


target/x86_64-unknown-linux-musl/release/slack-to-sns: slack-to-sns/src/main.rs slack-to-sns/Cargo.toml
	docker run --rm -i -v $(PWD):/home/rust/src ekidd/rust-musl-builder cargo build --release --target x86_64-unknown-linux-musl

target/slack-to-sns.zip: target/x86_64-unknown-linux-musl/release/slack-to-sns
	rm -f $@
	zip -j $@ $^
	ziptool $@ rename 0 bootstrap

target/x86_64-unknown-linux-musl/release/sqs-to-slack: sqs-to-slack/src/main.rs sqs-to-slack/Cargo.toml
	docker run --rm -i -v $(PWD):/home/rust/src ekidd/rust-musl-builder cargo build --release --target x86_64-unknown-linux-musl

target/sqs-to-slack.zip: target/x86_64-unknown-linux-musl/release/sqs-to-slack
	rm -f $@
	zip -j $@ $^
	ziptool $@ rename 0 bootstrap

.PHONY: build
build: target/slack-to-sns.zip target/sqs-to-slack.zip ## build zip file for AWS Lambda code

.PHONY: clean
clean: ## clean up build files
	cargo clean
	rm -f .output.yml

.output.yml: template.yml target/slack-to-sns.zip target/sqs-to-slack.zip
	aws cloudformation package \
		--region $(AWS_REGION) \
		--template-file template.yml \
		--s3-bucket $(AWS_S3_BUCKET) \
		--s3-prefix $(ENV)-slack-new-channel-notification \
		--output-template-file $@

.PHONY: package
package: .output.yml ## create AWS Lambda package and upload it to S3

.PHONY: deploy
deploy: .output.yml target/slack-to-sns.zip target/sqs-to-slack.zip ## deploy code to AWS Lambda
	aws cloudformation deploy \
		--region $(AWS_REGION) \
		--template-file .output.yml \
		--stack-name $(AWS_CLOUDFORMATION_STACK_NAME) \
		--capabilities CAPABILITY_IAM \
		--parameter-override Env=$(ENV)
