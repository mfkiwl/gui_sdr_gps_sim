#!/usr/bin/env python3
# -*- coding: utf-8 -*-

#
# SPDX-License-Identifier: GPL-3.0
#
# GNU Radio Python Flow Graph
# Title: GPS L1 C/A IQ File Analyzer
# Author: gui_sdr_gps_sim
# Description: Analyze a GPS L1 C/A IQ file written by gui_sdr_gps_sim (sc8 format)
# GNU Radio version: 3.10.12.0

from PyQt5 import Qt
from gnuradio import qtgui
from PyQt5 import QtCore
from gnuradio import blocks
from gnuradio import gr
from gnuradio.filter import firdes
from gnuradio.fft import window
import sys
import signal
from argparse import ArgumentParser
from gnuradio.eng_arg import eng_float, intx
from gnuradio import eng_notation
import sip
import threading



class gps_iq_file_analyzer(gr.top_block, Qt.QWidget):

    def __init__(self, iq_file="gps_signal.iq", samp_rate=3000000):
        gr.top_block.__init__(self, "GPS L1 C/A IQ File Analyzer", catch_exceptions=True)
        Qt.QWidget.__init__(self)
        self.setWindowTitle(f"GPS L1 C/A IQ File Analyzer — {iq_file}")
        qtgui.util.check_set_qss()
        try:
            self.setWindowIcon(Qt.QIcon.fromTheme('gnuradio-grc'))
        except BaseException as exc:
            print(f"Qt GUI: Could not set Icon: {str(exc)}", file=sys.stderr)
        self.top_scroll_layout = Qt.QVBoxLayout()
        self.setLayout(self.top_scroll_layout)
        self.top_scroll = Qt.QScrollArea()
        self.top_scroll.setFrameStyle(Qt.QFrame.NoFrame)
        self.top_scroll_layout.addWidget(self.top_scroll)
        self.top_scroll.setWidgetResizable(True)
        self.top_widget = Qt.QWidget()
        self.top_scroll.setWidget(self.top_widget)
        self.top_layout = Qt.QVBoxLayout(self.top_widget)
        self.top_grid_layout = Qt.QGridLayout()
        self.top_layout.addLayout(self.top_grid_layout)

        self.settings = Qt.QSettings("gnuradio/flowgraphs", "gps_iq_file_analyzer")

        try:
            geometry = self.settings.value("geometry")
            if geometry:
                self.restoreGeometry(geometry)
        except BaseException as exc:
            print(f"Qt GUI: Could not restore geometry: {str(exc)}", file=sys.stderr)
        self.flowgraph_started = threading.Event()

        ##################################################
        # Variables
        ##################################################
        self.samp_rate = samp_rate = samp_rate
        self.fft_avg = fft_avg = 0.2
        self.center_freq = center_freq = 1575420000

        ##################################################
        # Blocks
        ##################################################

        self._fft_avg_range = qtgui.Range(0.01, 1.0, 0.01, 0.2, 200)
        self._fft_avg_win = qtgui.RangeWidget(self._fft_avg_range, self.set_fft_avg, "FFT Average alpha", "counter_slider", float, QtCore.Qt.Horizontal)
        self.top_layout.addWidget(self._fft_avg_win)

        # Status label showing the file being analysed.
        self._file_label = Qt.QLabel(f"<b>IQ file:</b> {iq_file}  &nbsp;|&nbsp;  "
                                     f"<b>Rate:</b> {samp_rate/1e6:.3f} MSPS  &nbsp;|&nbsp;  "
                                     f"<b>Format:</b> sc8 (interleaved signed-8-bit I/Q) × 1/128")
        self._file_label.setAlignment(QtCore.Qt.AlignCenter)
        self.top_layout.addWidget(self._file_label)

        self.tabs = Qt.QTabWidget()
        self.tabs_widget_0 = Qt.QWidget()
        self.tabs_layout_0 = Qt.QBoxLayout(Qt.QBoxLayout.TopToBottom, self.tabs_widget_0)
        self.tabs_grid_layout_0 = Qt.QGridLayout()
        self.tabs_layout_0.addLayout(self.tabs_grid_layout_0)
        self.tabs.addTab(self.tabs_widget_0, 'FFT Spectrum')
        self.tabs_widget_1 = Qt.QWidget()
        self.tabs_layout_1 = Qt.QBoxLayout(Qt.QBoxLayout.TopToBottom, self.tabs_widget_1)
        self.tabs_grid_layout_1 = Qt.QGridLayout()
        self.tabs_layout_1.addLayout(self.tabs_grid_layout_1)
        self.tabs.addTab(self.tabs_widget_1, 'Waterfall')
        self.tabs_widget_2 = Qt.QWidget()
        self.tabs_layout_2 = Qt.QBoxLayout(Qt.QBoxLayout.TopToBottom, self.tabs_widget_2)
        self.tabs_grid_layout_2 = Qt.QGridLayout()
        self.tabs_layout_2.addLayout(self.tabs_grid_layout_2)
        self.tabs.addTab(self.tabs_widget_2, 'Time Domain')
        self.tabs_widget_3 = Qt.QWidget()
        self.tabs_layout_3 = Qt.QBoxLayout(Qt.QBoxLayout.TopToBottom, self.tabs_widget_3)
        self.tabs_grid_layout_3 = Qt.QGridLayout()
        self.tabs_layout_3.addLayout(self.tabs_grid_layout_3)
        self.tabs.addTab(self.tabs_widget_3, 'Constellation')
        self.top_grid_layout.addWidget(self.tabs, 1, 0, 1, 1)
        for r in range(1, 2):
            self.top_grid_layout.setRowStretch(r, 1)
        for c in range(0, 1):
            self.top_grid_layout.setColumnStretch(c, 1)

        # ── Waterfall sink ────────────────────────────────────────────────────────
        self.qtgui_waterfall_sink_x_0 = qtgui.waterfall_sink_c(
            4096, #size
            window.WIN_BLACKMAN_hARRIS, #wintype
            center_freq, #fc
            samp_rate, #bw
            "GPS L1 C/A — Waterfall", #name
            1, #number of inputs
            None # parent
        )
        self.qtgui_waterfall_sink_x_0.set_update_time(0.05)
        self.qtgui_waterfall_sink_x_0.enable_grid(True)
        self.qtgui_waterfall_sink_x_0.enable_axis_labels(True)

        labels = ['', '', '', '', '', '', '', '', '', '']
        colors = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        alphas = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]

        for i in range(1):
            if len(labels[i]) == 0:
                self.qtgui_waterfall_sink_x_0.set_line_label(i, "Data {0}".format(i))
            else:
                self.qtgui_waterfall_sink_x_0.set_line_label(i, labels[i])
            self.qtgui_waterfall_sink_x_0.set_color_map(i, colors[i])
            self.qtgui_waterfall_sink_x_0.set_line_alpha(i, alphas[i])

        self.qtgui_waterfall_sink_x_0.set_intensity_range(-40, 40)

        self._qtgui_waterfall_sink_x_0_win = sip.wrapinstance(self.qtgui_waterfall_sink_x_0.qwidget(), Qt.QWidget)
        self.tabs_layout_1.addWidget(self._qtgui_waterfall_sink_x_0_win)

        # ── Time domain sink (decimated ×100 to ~30 kHz effective rate) ──────────
        self.qtgui_time_sink_x_0 = qtgui.time_sink_c(
            1024, #size
            samp_rate / 100, #samp_rate
            "GPS L1 C/A — Time Domain (dec ×100)", #name
            1, #number of inputs
            None # parent
        )
        self.qtgui_time_sink_x_0.set_update_time(0.1)
        self.qtgui_time_sink_x_0.set_y_axis(-1.5, 1.5)
        self.qtgui_time_sink_x_0.set_y_label('Amplitude', "norm.")
        self.qtgui_time_sink_x_0.enable_tags(True)
        self.qtgui_time_sink_x_0.set_trigger_mode(qtgui.TRIG_MODE_FREE, qtgui.TRIG_SLOPE_POS, 0, 0, 0, "")
        self.qtgui_time_sink_x_0.enable_autoscale(True)
        self.qtgui_time_sink_x_0.enable_grid(True)
        self.qtgui_time_sink_x_0.enable_axis_labels(True)
        self.qtgui_time_sink_x_0.enable_control_panel(False)
        self.qtgui_time_sink_x_0.enable_stem_plot(False)

        labels = ['I', 'Q', 'Signal 3', 'Signal 4', 'Signal 5', 'Signal 6', 'Signal 7', 'Signal 8', 'Signal 9', 'Signal 10']
        widths = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1]
        colors = ['cyan', 'red', 'green', 'black', 'cyan', 'magenta', 'yellow', 'dark red', 'dark green', 'dark blue']
        alphas = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        styles = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1]
        markers = [-1, -1, -1, -1, -1, -1, -1, -1, -1, -1]

        for i in range(2):
            if len(labels[i]) == 0:
                if (i % 2 == 0):
                    self.qtgui_time_sink_x_0.set_line_label(i, "Re{{Data {0}}}".format(i // 2))
                else:
                    self.qtgui_time_sink_x_0.set_line_label(i, "Im{{Data {0}}}".format(i // 2))
            else:
                self.qtgui_time_sink_x_0.set_line_label(i, labels[i])
            self.qtgui_time_sink_x_0.set_line_width(i, widths[i])
            self.qtgui_time_sink_x_0.set_line_color(i, colors[i])
            self.qtgui_time_sink_x_0.set_line_style(i, styles[i])
            self.qtgui_time_sink_x_0.set_line_marker(i, markers[i])
            self.qtgui_time_sink_x_0.set_line_alpha(i, alphas[i])

        self._qtgui_time_sink_x_0_win = sip.wrapinstance(self.qtgui_time_sink_x_0.qwidget(), Qt.QWidget)
        self.tabs_layout_2.addWidget(self._qtgui_time_sink_x_0_win)

        # ── FFT / spectrum sink ───────────────────────────────────────────────────
        self.qtgui_freq_sink_x_0 = qtgui.freq_sink_c(
            4096, #size
            window.WIN_BLACKMAN_hARRIS, #wintype
            center_freq, #fc
            samp_rate, #bw
            "GPS L1 C/A — FFT Spectrum", #name
            1,
            None # parent
        )
        self.qtgui_freq_sink_x_0.set_update_time(0.05)
        self.qtgui_freq_sink_x_0.set_y_axis(-60, 10)
        self.qtgui_freq_sink_x_0.set_y_label('GPS L1 C/A Spectrum', 'dB')
        self.qtgui_freq_sink_x_0.set_trigger_mode(qtgui.TRIG_MODE_FREE, 0.0, 0, "")
        self.qtgui_freq_sink_x_0.enable_autoscale(True)
        self.qtgui_freq_sink_x_0.enable_grid(True)
        self.qtgui_freq_sink_x_0.set_fft_average(fft_avg)
        self.qtgui_freq_sink_x_0.enable_axis_labels(True)
        self.qtgui_freq_sink_x_0.enable_control_panel(False)
        self.qtgui_freq_sink_x_0.set_fft_window_normalized(True)

        labels = ['IQ stream', '', '', '', '', '', '', '', '', '']
        widths = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1]
        colors = ["cyan", "red", "green", "black", "cyan", "magenta", "yellow", "dark red", "dark green", "dark blue"]
        alphas = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]

        for i in range(1):
            if len(labels[i]) == 0:
                self.qtgui_freq_sink_x_0.set_line_label(i, "Data {0}".format(i))
            else:
                self.qtgui_freq_sink_x_0.set_line_label(i, labels[i])
            self.qtgui_freq_sink_x_0.set_line_width(i, widths[i])
            self.qtgui_freq_sink_x_0.set_line_color(i, colors[i])
            self.qtgui_freq_sink_x_0.set_line_alpha(i, alphas[i])

        self._qtgui_freq_sink_x_0_win = sip.wrapinstance(self.qtgui_freq_sink_x_0.qwidget(), Qt.QWidget)
        self.tabs_layout_0.addWidget(self._qtgui_freq_sink_x_0_win)

        # ── IQ constellation (scatter) sink ───────────────────────────────────────
        self.qtgui_const_sink_x_0 = qtgui.const_sink_c(
            8192, #size
            "GPS L1 C/A — Constellation", #name
            1, #number of inputs
            None # parent
        )
        self.qtgui_const_sink_x_0.set_update_time(0.1)
        self.qtgui_const_sink_x_0.set_y_axis(-1.5, 1.5)
        self.qtgui_const_sink_x_0.set_x_axis(-1.5, 1.5)
        self.qtgui_const_sink_x_0.set_trigger_mode(qtgui.TRIG_MODE_FREE, qtgui.TRIG_SLOPE_POS, 0, 0, "")
        self.qtgui_const_sink_x_0.enable_autoscale(True)
        self.qtgui_const_sink_x_0.enable_grid(True)
        self.qtgui_const_sink_x_0.enable_axis_labels(True)

        labels = ['GPS L1', '', '', '', '', '', '', '', '', '']
        widths = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1]
        colors = ["blue", "red", "green", "black", "cyan", "magenta", "yellow", "dark red", "dark green", "dark blue"]
        styles = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        markers = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        alphas = [0.4, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]

        for i in range(1):
            if len(labels[i]) == 0:
                self.qtgui_const_sink_x_0.set_line_label(i, "Data {0}".format(i))
            else:
                self.qtgui_const_sink_x_0.set_line_label(i, labels[i])
            self.qtgui_const_sink_x_0.set_line_width(i, widths[i])
            self.qtgui_const_sink_x_0.set_line_color(i, colors[i])
            self.qtgui_const_sink_x_0.set_line_style(i, styles[i])
            self.qtgui_const_sink_x_0.set_line_marker(i, markers[i])
            self.qtgui_const_sink_x_0.set_line_alpha(i, alphas[i])

        self._qtgui_const_sink_x_0_win = sip.wrapinstance(self.qtgui_const_sink_x_0.qwidget(), Qt.QWidget)
        self.tabs_layout_3.addWidget(self._qtgui_const_sink_x_0_win)

        # ── Source chain: IQ file → sc8 decoder → throttle ───────────────────────
        # File source reads the raw bytes written by gui_sdr_gps_sim IQ-file output.
        # repeat=True loops the file so the displays stay active indefinitely.
        self.blocks_file_source_0 = blocks.file_source(gr.sizeof_char * 1, iq_file, True)
        self.blocks_file_source_0.set_begin_tag(0)

        # interleaved_char_to_complex: (I_byte, Q_byte) → complex<float>
        # scale=1/128 maps the nominal ±127 range to approximately ±1.0
        self.blocks_interleaved_char_to_complex_0 = blocks.interleaved_char_to_complex(False, 1.0 / 128.0)

        # Throttle paces playback at the true sample rate so Qt sinks update
        # at wall-clock speed instead of racing through the file instantly.
        self.blocks_throttle_0 = blocks.throttle(gr.sizeof_gr_complex * 1, samp_rate, True)

        # 1-in-100 decimator so the time-domain view shows code-rate structure
        # (~30 kHz), not just a blur of 3 MSPS carrier.
        self.blocks_keep_one_in_n_0 = blocks.keep_one_in_n(gr.sizeof_gr_complex * 1, 100)

        ##################################################
        # Connections
        ##################################################
        self.connect((self.blocks_file_source_0, 0), (self.blocks_interleaved_char_to_complex_0, 0))
        self.connect((self.blocks_interleaved_char_to_complex_0, 0), (self.blocks_throttle_0, 0))
        self.connect((self.blocks_throttle_0, 0), (self.blocks_keep_one_in_n_0, 0))
        self.connect((self.blocks_throttle_0, 0), (self.qtgui_const_sink_x_0, 0))
        self.connect((self.blocks_throttle_0, 0), (self.qtgui_freq_sink_x_0, 0))
        self.connect((self.blocks_throttle_0, 0), (self.qtgui_waterfall_sink_x_0, 0))
        self.connect((self.blocks_keep_one_in_n_0, 0), (self.qtgui_time_sink_x_0, 0))

    def closeEvent(self, event):
        self.settings = Qt.QSettings("gnuradio/flowgraphs", "gps_iq_file_analyzer")
        self.settings.setValue("geometry", self.saveGeometry())
        self.stop()
        self.wait()
        event.accept()

    def get_samp_rate(self):
        return self.samp_rate

    def set_samp_rate(self, samp_rate):
        self.samp_rate = samp_rate
        self.blocks_throttle_0.set_sample_rate(self.samp_rate)
        self.qtgui_freq_sink_x_0.set_frequency_range(self.center_freq, self.samp_rate)
        self.qtgui_time_sink_x_0.set_samp_rate(self.samp_rate / 100)
        self.qtgui_waterfall_sink_x_0.set_frequency_range(self.center_freq, self.samp_rate)

    def get_fft_avg(self):
        return self.fft_avg

    def set_fft_avg(self, fft_avg):
        self.fft_avg = fft_avg
        self.qtgui_freq_sink_x_0.set_fft_average(self.fft_avg)

    def get_center_freq(self):
        return self.center_freq

    def set_center_freq(self, center_freq):
        self.center_freq = center_freq
        self.qtgui_freq_sink_x_0.set_frequency_range(self.center_freq, self.samp_rate)
        self.qtgui_waterfall_sink_x_0.set_frequency_range(self.center_freq, self.samp_rate)


def argument_parser():
    parser = ArgumentParser(description="GPS L1 C/A IQ file signal analyser (GNU Radio)")
    parser.add_argument(
        "iq_file", metavar="IQ_FILE", nargs="?", default="gps_signal.iq",
        help="Path to the IQ file written by gui_sdr_gps_sim (sc8 format). Default: gps_signal.iq"
    )
    parser.add_argument(
        "--rate", dest="samp_rate", type=eng_float, default=eng_notation.num_to_str(float(3000000)),
        help="Sample rate in Hz — must match the app's 'Frequency' setting (default: 3M)"
    )
    return parser


def main(top_block_cls=gps_iq_file_analyzer, options=None):
    if options is None:
        options = argument_parser().parse_args()

    qapp = Qt.QApplication(sys.argv)

    tb = top_block_cls(iq_file=options.iq_file, samp_rate=options.samp_rate)

    tb.start()
    tb.flowgraph_started.set()

    tb.show()

    def sig_handler(sig=None, frame=None):
        tb.stop()
        tb.wait()
        Qt.QApplication.quit()

    signal.signal(signal.SIGINT, sig_handler)
    signal.signal(signal.SIGTERM, sig_handler)

    timer = Qt.QTimer()
    timer.start(500)
    timer.timeout.connect(lambda: None)

    qapp.exec_()


if __name__ == '__main__':
    main()
