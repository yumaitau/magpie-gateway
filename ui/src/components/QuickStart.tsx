import { Link } from '@tanstack/react-router';
import { Check } from 'lucide-react';

export function QuickStart(props: { hasModels: boolean; busy: boolean; onAddModel: () => void }) {
	return (
		<section className="quick-start" aria-labelledby="quick-start-title">
			<div className="quick-start-heading">
				<h2 id="quick-start-title">
					{props.hasModels ? 'Your next steps' : 'Start with your first model'}
				</h2>
				<p>Connect a provider, try a prompt, then use the gateway in your app.</p>
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
						2
					</span>
					<div>
						<h3>Try a prompt</h3>
						<p>Use the playground to check that your model responds.</p>
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
						3
					</span>
					<div>
						<h3>Connect your app</h3>
						<p>Pick your client and copy its connection settings.</p>
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
	);
}
