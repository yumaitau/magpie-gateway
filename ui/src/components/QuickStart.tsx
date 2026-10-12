import { Link } from '@tanstack/react-router';
import { Check } from 'lucide-react';

import { isMagpie } from '@/magpie';
import { useSetupProgress } from '@/setupProgress';

export function QuickStart(props: {
	hasModels: boolean;
	modelNames: string[];
	busy: boolean;
	onAddModel: () => void;
}) {
	const progress = useSetupProgress();
	const tested = Boolean(progress.testedModel && props.modelNames.includes(progress.testedModel));
	const connected = Boolean(
		progress.connectedModel && props.modelNames.includes(progress.connectedModel)
	);
	const complete = props.hasModels && tested && connected;
	return (
		<details className="setup-checklist" key={String(complete)} open={!complete}>
			<summary>{complete ? 'Setup complete' : 'Quick start'}</summary>
			<section className="quick-start" aria-labelledby="quick-start-title">
				<div className="quick-start-heading">
					<h2 id="quick-start-title">
						{props.hasModels ? 'Your next steps' : 'Start with your first model'}
					</h2>
					<p>Connect a provider, try a prompt, then use the gateway in your app.</p>
					{isMagpie && (
						<Link to="/help/$topic" params={{ topic: 'start' }}>
							Get setup prompt.md and step-by-step help
						</Link>
					)}
				</div>
				<ol className="quick-start-steps">
					<li>
						<span className="quick-start-number" aria-hidden="true">
							{props.hasModels ? <Check size={18} /> : '1'}
						</span>
						<div>
							<h3>Add a model</h3>
							<p>
								{props.hasModels
									? 'A model is configured. Try a request to check the connection.'
									: 'Choose your provider and add its credentials. Routing defaults are filled in for you.'}
							</p>
						</div>
						<button
							className={`button${props.hasModels ? '' : ' primary'}`}
							type="button"
							disabled={props.busy}
							onClick={props.onAddModel}
						>
							{props.busy
								? 'Setting up…'
								: props.hasModels
									? 'Add another model'
									: 'Add your first model'}
						</button>
					</li>
					<li>
						<span className="quick-start-number" aria-hidden="true">
							{tested ? <Check size={18} /> : '2'}
						</span>
						<div>
							<h3>Try a prompt</h3>
							<p>
								{tested
									? 'Your model returned a successful response.'
									: 'Use the playground to test your connection.'}
							</p>
						</div>
						{props.hasModels ? (
							<Link className="button primary" to="/llm/playground">
								Try your model
							</Link>
						) : (
							<span className="muted-copy">Add a model first</span>
						)}
					</li>
					<li>
						<span className="quick-start-number" aria-hidden="true">
							{connected ? <Check size={18} /> : '3'}
						</span>
						<div>
							<h3>Connect your app</h3>
							<p>
								{connected
									? 'You confirmed your app connection.'
									: 'Pick your client and copy its connection settings.'}
							</p>
						</div>
						{props.hasModels ? (
							<Link className="button" to="/llm/client-setup">
								Connect your app
							</Link>
						) : (
							<span className="muted-copy">Add a model first</span>
						)}
					</li>
				</ol>
				<p className="muted-copy">
					Need MCP tools or HTTP routing? Open Gateway settings below. Policies and specialist tools
					are under Advanced in the navigation.
				</p>
			</section>
		</details>
	);
}
