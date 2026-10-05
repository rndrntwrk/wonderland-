// Test-only scheduling adapter. Original Task/TaskCompletionSource/ContinueWith
// implementations remain in use; the original EODPersist source is unchanged.
using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;

namespace EodOracle
{
    public sealed class QueuedScheduler : TaskScheduler
    {
        private readonly Queue<Task> Ready = new Queue<Task>();
        protected override IEnumerable<Task> GetScheduledTasks() { return Ready.ToArray(); }
        protected override bool TryExecuteTaskInline(Task task, bool wasQueued) { return false; }
        protected override void QueueTask(Task task)
        {
            if (Ready.Count >= 4096) throw new InvalidOperationException("scheduled task limit");
            Ready.Enqueue(task);
        }

        public void Run(Action action)
        {
            // Running the invocation as a Task makes TaskScheduler.Current this
            // scheduler when original source creates each continuation.
            Task.Factory.StartNew(action, CancellationToken.None, TaskCreationOptions.None, this);
            int count = 0;
            while (Ready.Count != 0)
            {
                if (++count > 4096) throw new InvalidOperationException("task pump limit");
                var task = Ready.Dequeue();
                if (!TryExecuteTask(task)) throw new InvalidOperationException("task was not executed");
                if (task.IsFaulted) throw task.Exception.GetBaseException();
            }
        }
    }
}
