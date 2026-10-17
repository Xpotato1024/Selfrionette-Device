/**
 *
 * HX711 library for Arduino
 * https://github.com/bogde/HX711
 *
 * MIT License
 * (c) 2018 Bogdan Necula
 *
**/
#ifndef HX711_h
#define HX711_h

#if ARDUINO >= 100
#include "Arduino.h"
#else
#include "WProgram.h"
#endif

class HX711
{
	private:
		byte PD_SCK;
		byte DOUT;
		byte GAIN;
		long read_ready();

	public:
		HX711();
		virtual ~HX711();

		void begin(byte dout, byte pd_sck, byte gain = 128);
		bool is_ready();
		bool wait_ready_retry(int retries = 3, unsigned long delay_ms = 0);
		bool wait_ready_timeout(unsigned long timeout = 1000, unsigned long delay_ms = 0);
		void set_gain(byte gain = 128);
		bool read_raw_if_ready(long* value);
		void power_down();
		void power_up();
};

#endif
