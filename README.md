## **R**ust **A**coustic **E**cho **C**ancellation

A very simple experiment on acoustic echo cancellation (AEC) using a Normalized
Least-mean-fourth (NLMF) filter as described
[here](https://matousc89.github.io/padasip/sources/filters/nlmf.html#zerguine2000convergence).
Project is in very rudimentary state and will most likely not be continued,
however if you would like to experiment with acoustic echo cancellation or audio
processing in rust this might be a good starting point. Currently you may
compile the main binary with `cargo build --release` (note the release flag is
quite necessary to achieve low audio latencies) and try it out, although you
should keep the following in mind. The purpose of AEC is to reduce the feedback
of a known output into the microphone input, and subsequently send this signal
somewhere else. For example, if you are in a voice call, the known output is the
incoming audio of your interlocutor which you don't want to be sent back through
your microphone, therefore you want the software to remove this known output
from the input microphone and then send the resulting audio to your voice chat
software. The critical thing here is how to do this routing of audio back into
the voice communication; in Windows I have used the proprietary software
[Virtual Audio Cable](https://vb-audio.com/Cable/), I am unaware of how one
would perform this in Linux, but you might want to look into JACK audio or the
new PipeWire. To use `raec` you need these three things: an audio output you
want to subtract from your input (referred to as capture device), your
microphone input (referred to as microphone device), and an audio device to send
the filtered signal to (referred to as output device). You can check `raec
--help` and `raec --list` for how to pass this information into the program.
